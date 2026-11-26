// Standalone JavaScript source, lowered without executing user code.
// It parses, never executes, the module and uses the shared PERS kernel.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { invokeKernel } from './invoke-kernel.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import ts from 'typescript';
import { lowerExpression, parsePlainModule, unwrapParens as unwrap } from './ir.mjs';
import { compileTypedTuple } from './module-tuple.mjs';
import { compileTypedListener } from './module-listener.mjs';
import { typedModuleAst } from './typed-module.mjs';
import { localDirectives } from './local-directives.mjs';
import { projectSources } from './project-source.mjs';

const domains = new Map([['number', ['int', 'number']], ['boolean', ['boolean', 'bool']]]);
const scalar = domain => domains.get(domain)?.[1];
const readonlyVector = 'ReadonlyArray<number>';
const readonlyMap = 'ReadonlyMap<string,string>';
const nullableText = 'string|null';
const parameter = domain => scalar(domain) ?? (domain === readonlyVector ? 'number[]'
  : domain === nullableText ? 'text?' : null);
const carrier = domain => domain === readonlyVector ? 'int[]'
  : domain === nullableText ? 'String' : domains.get(domain)[0];
const valueType = domain => domain === readonlyVector ? 'vector<number>' : parameter(domain);
function declaredParameter(node, sf) {
  if (node && ts.isTypeReferenceNode(node)
      && ts.isIdentifier(node.typeName) && node.typeName.text === 'ReadonlyArray'
      && node.typeArguments?.length === 1
      && node.typeArguments[0].kind === ts.SyntaxKind.NumberKeyword) return readonlyVector;
  if (node && ts.isUnionTypeNode(node) && node.types.length === 2
      && node.types.some(type => type.kind === ts.SyntaxKind.StringKeyword)
      && node.types.some(type => ts.isLiteralTypeNode(type)
        && type.literal.kind === ts.SyntaxKind.NullKeyword)) return nullableText;
  return node?.getText(sf);
}
function declaredOrderedMap(node) {
  return node && ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName)
    && node.typeName.text === 'ReadonlyMap' && node.typeArguments?.length === 2
    && node.typeArguments.every(type => type.kind === ts.SyntaxKind.StringKeyword);
}
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
  const top = declaration ? declaration.initializer : first.expression;
  if (!top) return null;
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

function mapEntries(node, names) {
  node = unwrap(node);
  if (!ts.isNewExpression(node) || !ts.isIdentifier(node.expression)
      || node.expression.text !== 'Map' || node.arguments?.length !== 1) {
    fail('readonly Map requires one built-in Map construction');
  }
  const rows = unwrap(node.arguments[0]);
  if (!ts.isArrayLiteralExpression(rows)) fail('readonly Map requires literal entry assembly');
  const seen = new Set();
  return rows.elements.map(row => {
    const candidate = ts.isSpreadElement(row) && unwrap(row.expression);
    if (!candidate || !ts.isConditionalExpression(candidate)) fail('readonly Map requires explicitly null-guarded entries');
    const cond = unwrap(candidate.condition);
    if (!ts.isBinaryExpression(cond)
        || ![ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(cond.operatorToken.kind)) {
      fail('readonly Map requires an exact null guard');
    }
    const left = unwrap(cond.left), right = unwrap(cond.right);
    const subject = left.kind === ts.SyntaxKind.NullKeyword ? right
      : right.kind === ts.SyntaxKind.NullKeyword ? left : null;
    const nullFirst = cond.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken;
    const empty = unwrap(nullFirst ? candidate.whenTrue : candidate.whenFalse);
    const present = unwrap(nullFirst ? candidate.whenFalse : candidate.whenTrue);
    if (!ts.isArrayLiteralExpression(empty) || empty.elements.length || !ts.isArrayLiteralExpression(present)
        || present.elements.length !== 1) fail('readonly Map requires omission only in the null branch');
    const entry = unwrap(present.elements[0]);
    if (!ts.isArrayLiteralExpression(entry) || entry.elements.length !== 2 || !ts.isStringLiteral(entry.elements[0])) {
      fail('readonly Map requires literal text keys');
    }
    const [key, raw] = entry.elements;
    const value = unwrap(raw);
    if (!subject || !ts.isIdentifier(subject) || !ts.isIdentifier(value) || value.text !== subject.text
        || names.get(value.text)?.type !== 'text?') fail('readonly Map requires each null guard to protect the inserted value');
    if (seen.has(key.text)) fail('readonly Map requires unique literal keys');
    seen.add(key.text);
    return {key: key.text, input: names.get(value.text).name};
  });
}

function compile(source, file, entry, physicalPath) {
  if (file.endsWith('.ts')) {
    const {sf, fn} = typedModuleAst(source, physicalPath, entry);
    const isListener = fn.type?.kind === ts.SyntaxKind.VoidKeyword;
    const typed = isListener ? compileTypedListener(physicalPath, sf, fn)
      : compileTypedTuple(physicalPath, sf, fn);
    const params = typed.params.map((name, position) => ({publicName: name, name, position, domain: 'boolean'}));
    const returns = isListener ? 'void' : 'number';
    return projectInput(file, entry, params, returns, false, returns, typed.result, typed.excerpt, typed);
  }
  const sf = parsePlainModule(file, source, fail);
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
  const declaredReturn = ts.getJSDocReturnType(fn);
  const orderedMap = declaredOrderedMap(declaredReturn);
  const returnDomain = orderedMap ? readonlyMap : declaredParameter(declaredReturn, sf);
  if (!orderedMap && (!parameter(returnDomain) || returnDomain === nullableText)) fail('expected declared business scalar or non-null readonly Vector return type');
  if (orderedMap && sf.statements.some(declaration => declaration.name?.text === 'Map')) {
    fail('readonly Map requires an unshadowed built-in Map');
  }
  const names = new Map();
  const params = fn.parameters.map((param, position) => {
    if (!ts.isIdentifier(param.name) || param.initializer || param.dotDotDotToken || param.questionToken) fail('expected named required parameters');
    const [tag, ...more] = ts.getJSDocParameterTags(param);
    const domain = declaredParameter(tag?.typeExpression?.type, sf);
    if (more.length || !parameter(domain) || (!orderedMap && domain === nullableText)) fail('expected declared business scalar or non-null readonly Vector for ' + param.name.text);
    if (orderedMap && param.name.text === 'Map') fail('readonly Map requires an unshadowed built-in Map');
    if (names.has(param.name.text)) fail('duplicate parameter ' + param.name.text);
    const binding = param.name.text;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(binding)) fail('unsupported business parameter name ' + binding);
    names.set(param.name.text, {name: binding, type: parameter(domain)});
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
    if (expr.op === 'read') {
      const [name, field, ...rest] = expr.target;
      const binding = names.get(name);
      if (field !== 'length' || rest.length || binding?.type !== 'number[]') {
        fail('length requires a direct declared non-null readonly Vector');
      }
      return {type: 'number', node: internal('value.field', {value: internal('var', {name: binding.name}), name: 'length'})};
    }
    if (expr.op === 'binary' && ['>', '<', '==='].includes(expr.operator)) {
      const left = expression(expr.left), right = expression(expr.right);
      if (left.type !== 'number' || right.type !== 'number') fail('comparison requires two business numbers');
      const [a, b] = expr.operator === '<' ? [right, left] : [left, right];
      return {type: 'bool', node: internal(expr.operator === '===' ? 'eq' : 'gt', {left: a.node, right: b.node})};
    }
    if (expr.op === 'array') {
      if (expr.items.some(item => item.op === 'opaque')) fail('readonly Vector literal requires a closed list of numbers');
      const items = expr.items.map(expression);
      if (items.some(item => item.type !== 'number')) fail('readonly Vector literal requires a closed list of numbers');
      return {type: 'number[]', node: internal('value.number_vector', {items: items.map(item => item.node)})};
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
  const returns = orderedMap ? 'map<text,text>' : parameter(returnDomain);
  const result = orderedMap
    ? {type: returns, node: internal('value.ordered_text_map', {entries: mapEntries(last.expression, names)})}
    : expression(lowerExpression(last.expression, sf));
  if (result.type !== returns) fail('return value differs from declared domain');
  return projectInput(file, entry, params, returnDomain, orderedMap, returns, result.node, fn.getText(sf));
}

function projectInput(file, entry, params, returnDomain, orderedMap, returns, body, excerpt, typed = {}) {
  const binding = 'Entry.run';
  const moduleFile = file.endsWith('.ts') ? 'Module.ts' : 'Module.js';
  const javaParams = params.map(param => carrier(param.domain));
  const declarations = javaParams.map((type, index) => type + ' ' + params[index].name);
  const resultTypes = returnDomain === readonlyVector ? {ReadonlyNumbers: 'vector<number>'}
    : orderedMap ? {ReadonlyLabels: returns} : {};
  const returnCarrier = returnDomain === 'void' ? 'void' : Object.keys(resultTypes)[0] ?? carrier(returnDomain);
  const javaSource = Object.keys(resultTypes).map(name => 'class ' + name + ' {}\n').join('') +
    'class Entry { static native ' + returnCarrier + ' run(' + declarations.join(', ') + '); }\n';
  return {carrier: javaSource, binding, moduleFile, project: {
    schema: 'normalizer.kernel-input/1', source: 'Carrier.java', source_text: javaSource, entries: [binding],
    result_value_types: resultTypes,
    ...(typed.ports && {ports: typed.ports}),
    parameter_value_types: {run: Object.fromEntries(params.filter(p => p.domain === readonlyVector).map(p => [p.name, 'vector<number>']))},
    entry_contexts: {[binding]: {
      function: file + '#' + entry, returns,
      ...(typed.externalResults && {external_results: typed.externalResults}),
      parameters: Object.fromEntries(params.map(param => [param.publicName, {position: param.position, type: parameter(param.domain)}])),
    }},
    functions: {run: {
      java: binding + '(' + javaParams.join(',') + ')',
      params: params.map(param => ({name: param.name, type: valueType(param.domain)})),
      returns: returns === 'void' ? 'unit' : resultTypes[returnCarrier] ?? returns, origin: {path: moduleFile, excerpt},
      body,
    }},
  }};
}

function publishKernelFailure(result) {
  process.stderr.write(result.stderr || '[NORM-JS-MODULE] kernel failed without a diagnostic\n');
  process.exitCode = result.status ?? 1;
}

try {
  const [kernel, rootArg, file, entry] = process.argv.slice(2);
  if (!kernel || !rootArg || !file || !entry || !/\.(js|ts)$/.test(file)) fail('expected PROJECT_ROOT, a relative .js or .ts file, and an exported function');
  const { declared, name, path } = projectSources(rootArg)(file);
  const source = readFileSync(path, 'utf8');
  const {carrier, binding, project, closure, moduleFile} = compile(source, name, entry, declared);
  if (closure) {
    const result = invokeKernel(kernel, ['--validate-standalone-closure'], JSON.stringify(closure));
    if (result.status === 0 || result.stdout) fail('expected a kernel closure failure with no result');
    publishKernelFailure(result);
  } else {
    const temp = mkdtempSync(join(tmpdir(), 'normalizer-js-module-'));
    try {
      writeFileSync(join(temp, 'Carrier.java'), carrier);
      writeFileSync(join(temp, moduleFile), source);
      const projectPath = join(temp, 'Project.json');
      writeFileSync(projectPath, JSON.stringify(project));
      const result = invokeKernel(kernel, ['--project', projectPath, binding]);
      if (result.status !== 0) {
        publishKernelFailure(result);
      } else {
        process.stdout.write(result.stdout);
      }
    } finally { rmSync(temp, {recursive: true, force: true}); }
  }
} catch (error) {
  process.stderr.write(error.message + '\n');
  process.exitCode = 1;
}
