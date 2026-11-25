// Standalone JavaScript source, lowered without executing user code.
// It parses, never executes, the module and uses the shared PERS kernel.
import { mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { isAbsolute, join, relative, resolve } from 'node:path';
import ts from 'typescript';
import { lowerExpression } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';

const domains = new Map([['number', ['int', 'number']], ['boolean', ['boolean', 'bool']]]);
const scalar = domain => domains.get(domain)?.[1];
const internal = (op, fields) => ({op, ...fields});
function fail(detail) { throw new Error('[NORM-JS-MODULE] ' + detail); }

// In this bounded source prefix, the entry immediately evaluates a normal
// invocation on an ordinary caller-supplied parameter. This is actual call
// evidence, not an invented port or a synthetic scalar result for the kernel.
function leadingParameterCall(fn) {
  const params = [];
  for (const param of fn.parameters) {
    if (!ts.isIdentifier(param.name) || param.initializer || param.dotDotDotToken || param.questionToken) return null;
    if (params.includes(param.name.text)) return null;
    params.push(param.name.text);
  }
  const nodes = [...fn.body.statements];
  const first = nodes[0];
  if (!first) return null;
  const declared = new Set(params);
  for (const [i, statement] of nodes.entries()) {
    if (ts.isVariableStatement(statement) && statement.declarationList.flags & ts.NodeFlags.Const) {
      for (const declaration of statement.declarationList.declarations) {
        if (!ts.isIdentifier(declaration.name) || !declaration.initializer || declared.has(declaration.name.text)) return null;
        declared.add(declaration.name.text);
      }
    } else if (!(ts.isReturnStatement(statement) && i === nodes.length - 1)
      && !(ts.isExpressionStatement(statement) && i === 0)) return null;
  }
  const declaration = ts.isVariableStatement(first) ? first.declarationList.declarations[0] : null;
  const top = declaration && ts.isIdentifier(declaration.name) ? declaration.initializer
    : ts.isReturnStatement(first) || ts.isExpressionStatement(first) ? first.expression : null;
  if (!top) return null;
  const unwrap = node => {
    while (ts.isParenthesizedExpression(node)) node = node.expression;
    return node;
  };
  const expression = unwrap(top);
  const awaited = ts.isAwaitExpression(expression);
  const call = awaited ? unwrap(expression.expression) : expression;
  if (!ts.isCallExpression(call) || call.questionDotToken || call.arguments.length !== 0) return null;
  const callee = unwrap(call.expression);
  if (!ts.isPropertyAccessExpression(callee) || callee.questionDotToken || !ts.isIdentifier(callee.name)) return null;
  const receiver = unwrap(callee.expression);
  if (!ts.isIdentifier(receiver) || !params.includes(receiver.text)) return null;
  return { parameters: params, call: {receiver: receiver.text, member: callee.name.text, awaited} };
}

function compile(source, file, entry) {
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  if (sf.parseDiagnostics.length) fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  const exported = sf.statements.filter(node => ts.isFunctionDeclaration(node) && node.name?.text === entry
    && node.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword));
  if (exported.length !== 1 || !exported[0].body) fail('expected one defined exported function: ' + entry);
  const fn = exported[0];
  // Imports, executable top-level initialization and other module state
  // cannot be ignored by a closed standalone scalar adapter.
  if (sf.statements.some(node => !ts.isFunctionDeclaration(node))) fail('standalone scalar modules require closed function declarations');
  localDirectives(sf, fn.body).requireConsumed();
  if (fn.asteriskToken) fail('generator entry requires a closed completion contract: ' + entry);
  const call = leadingParameterCall(fn);
  if (call) return { closure: { schema: 'normalizer.standalone-closure/1', file, entry, ...call } };
  if (fn.modifiers.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) {
    fail('async entry requires a closed completion contract: ' + entry);
  }
  const returnDomain = ts.getJSDocReturnType(fn)?.getText(sf);
  if (!scalar(returnDomain)) fail('expected declared business scalar return type');
  const names = new Map();
  const params = fn.parameters.map((param, position) => {
    if (!ts.isIdentifier(param.name) || param.initializer || param.dotDotDotToken || param.questionToken) fail('expected named required parameters');
    const [tag, ...more] = ts.getJSDocParameterTags(param);
    const domain = tag?.typeExpression?.type.getText(sf);
    if (more.length || !scalar(domain)) fail('expected declared business scalar for ' + param.name.text);
    if (names.has(param.name.text)) fail('duplicate parameter ' + param.name.text);
    const binding = param.name.text;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(binding)) fail('unsupported business parameter name ' + binding);
    names.set(param.name.text, {name: binding, type: scalar(domain)});
    return {publicName: param.name.text, name: binding, domain, position};
  });
  function expression(expr) {
    if (expr.op === 'var') {
      const binding = names.get(expr.name);
      if (!binding) fail('unclosed value ' + expr.name);
      return {type: binding.type, node: internal('var', {name: binding.name})};
    }
    if (expr.op === 'literal' && (expr.type === 'boolean' || expr.type === 'number' && Number.isSafeInteger(expr.value))) {
      const type = scalar(expr.type);
      return {type, node: internal('const', {type, value: expr.value})};
    }
    if (expr.op === 'binary' && ['+', '-'].includes(expr.operator)) {
      const left = expression(expr.left), right = expression(expr.right);
      if (left.type !== 'number' || right.type !== 'number') fail('exact arithmetic requires two business numbers');
      return {type: 'number', node: internal(expr.operator === '+' ? 'add' : 'subtract', {left: left.node, right: right.node})};
    }
    if (expr.op === 'select') {
      const test = expression(expr.condition), yes = expression(expr.whenTrue), no = expression(expr.whenFalse);
      if (test.type !== 'bool' || yes.type !== no.type) fail('incompatible conditional types');
      return {type: yes.type, node: internal('if', {cond: test.node, then: yes.node, else: no.node})};
    }
    fail('unsupported standalone scalar operation ' + expr.op);
  }
  const [last, ...remaining] = fn.body.statements;
  if (remaining.length || !last || !ts.isReturnStatement(last) || !last.expression) fail('expected one closed return expression');
  const result = expression(lowerExpression(last.expression, sf));
  const returns = scalar(returnDomain);
  if (result.type !== returns) fail('return value differs from declared domain');
  const binding = 'Entry.run';
  const javaParams = params.map(param => domains.get(param.domain)[0]);
  const declarations = params.map((param, i) => javaParams[i] + ' ' + param.name);
  const carrier = 'class Entry { static native ' + domains.get(returnDomain)[0] + ' run(' + declarations.join(', ') + '); }\n';
  return {carrier, binding, project: {
    schema: 'normalizer.kernel-input/1', source: 'Carrier.java', source_text: carrier, entries: [binding],
    entry_contexts: {[binding]: {
      function: file + '#' + entry, returns,
      parameters: Object.fromEntries(params.map(param => [param.publicName, {position: param.position, type: scalar(param.domain)}])),
    }},
    functions: {run: {
      java: binding + '(' + javaParams.join(',') + ')',
      params: params.map(param => ({name: param.name, type: scalar(param.domain)})),
      returns, origin: {path: 'Module.js', excerpt: fn.getText(sf)},
      body: result.node,
    }},
  }};
}

try {
  const [kernel, rootArg, file, entry] = process.argv.slice(2);
  if (!kernel || !rootArg || !file || !entry || isAbsolute(file) || !file.endsWith('.js')) fail('expected PROJECT_ROOT, a relative .js file, and an exported function');
  const root = realpathSync(rootArg);
  const path = realpathSync(resolve(root, file));
  const name = relative(root, path);
  if (name === '..' || name.startsWith('../') || isAbsolute(name)) fail('module path must remain in the project');
  const source = readFileSync(path, 'utf8');
  const {carrier, binding, project, closure} = compile(source, name, entry);
  if (closure) {
    const result = spawnSync(kernel, ['--validate-standalone-closure'], {input: JSON.stringify(closure), encoding: 'utf8'});
    if (result.error) throw result.error;
    if (result.status === 0 || result.stdout) fail('expected a kernel closure failure with no result');
    process.stderr.write(result.stderr || '[NORM-JS-MODULE] kernel failed without a diagnostic\n');
    process.exitCode = result.status ?? 1;
  } else {
    const temp = mkdtempSync(join(tmpdir(), 'normalizer-js-module-'));
    try {
      writeFileSync(join(temp, 'Carrier.java'), carrier);
      writeFileSync(join(temp, 'Module.js'), source);
      const projectPath = join(temp, 'Project.yaml');
      writeFileSync(projectPath, JSON.stringify(project));
      const result = spawnSync(kernel, ['--project', projectPath, binding], {encoding: 'utf8'});
      if (result.error) throw result.error;
      if (result.status !== 0) {
        process.stderr.write(result.stderr || '[NORM-JS-MODULE] kernel failed without a diagnostic\n');
        process.exitCode = result.status ?? 1;
      } else {
        process.stdout.write(result.stdout);
      }
    } finally { rmSync(temp, {recursive: true, force: true}); }
  }
} catch (error) {
  process.stderr.write(error.message + '\n');
  process.exitCode = 1;
}
