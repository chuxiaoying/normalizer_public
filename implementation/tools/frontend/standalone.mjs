// Standalone TypeScript source, checked and lowered without executing user code.
// It parses, never executes, the module and uses the shared PERS kernel.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { invokeKernel } from './invoke-kernel.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import ts from 'typescript';
import { lowerExpression, unwrapParens as unwrap } from './ir.mjs';
import { compileTypedTuple } from './module-tuple.mjs';
import { compileTypedListener } from './module-listener.mjs';
import { checkedTypeScript, failTypedModule, typedModuleAst } from './typed-module.mjs';
import { entryLocator } from './entry-locator.mjs';
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
  if (node && ts.isTypeOperatorNode(node) && node.operator === ts.SyntaxKind.ReadonlyKeyword &&
      ts.isArrayTypeNode(node.type) && node.type.elementType.kind === ts.SyntaxKind.NumberKeyword) {
    return readonlyVector;
  }
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
const fail = failTypedModule;

// In this bounded source prefix, the entry immediately evaluates a normal
// invocation on an ordinary caller-supplied parameter. This is actual call
// evidence, not an invented port or a synthetic scalar result for the kernel.
function namedParameters(parameters) {
  const names = new Set();
  for (const param of parameters) {
    if (!ts.isIdentifier(param.name) || param.initializer || param.dotDotDotToken || param.questionToken)
      return {error: 'expected named required parameters'};
    names.add(param.name.text);
  }
  return {names};
}

function leadingParameterCall(fn, params) {
  const nodes = fn.body.statements, first = nodes[0];
  const top = first && (ts.isVariableStatement(first)
    ? first.declarationList.declarations[0]?.initializer : first.expression);
  const declared = new Set(params);
  for (const [index, statement] of nodes.entries()) {
    if (ts.isReturnStatement(statement) && index === nodes.length - 1) break;
    if (!ts.isVariableStatement(statement) || !(statement.declarationList.flags & ts.NodeFlags.Const)) return;
    for (const {name, initializer} of statement.declarationList.declarations) {
      if (!ts.isIdentifier(name) || !initializer || declared.has(name.text)) return;
      declared.add(name.text);
    }
  }
  if (!top) return;
  const expression = unwrap(top);
  const awaited = ts.isAwaitExpression(expression);
  const call = awaited ? unwrap(expression.expression) : expression;
  if (!ts.isCallExpression(call) || call.questionDotToken || call.arguments.length) return;
  const callee = unwrap(call.expression);
  if (!ts.isPropertyAccessExpression(callee) || callee.questionDotToken || !ts.isIdentifier(callee.name)) return;
  const receiver = unwrap(callee.expression);
  if (ts.isIdentifier(receiver) && params.has(receiver.text)) {
    return { parameters: [...params], call: {receiver: receiver.text, member: callee.name.text, awaited} };
  }
}

function mapEntries(node, names, fail) {
  node = unwrap(node);
  const argument = ts.isNewExpression(node) && ts.isIdentifier(node.expression)
    && node.expression.text === 'Map' && node.arguments?.length === 1 && unwrap(node.arguments[0]);
  if (!argument || !ts.isArrayLiteralExpression(argument)) {
    fail('readonly Map requires one built-in Map construction with literal entry assembly');
  }
  const seen = new Set();
  return argument.elements.map(row => {
    const candidate = ts.isSpreadElement(row) && unwrap(row.expression);
    const conditional = candidate && ts.isConditionalExpression(candidate);
    const cond = conditional && unwrap(candidate.condition);
    const operator = cond && ts.isBinaryExpression(cond) && cond.operatorToken.kind;
    const nullFirst = operator === ts.SyntaxKind.EqualsEqualsEqualsToken;
    const empty = conditional && unwrap(nullFirst ? candidate.whenTrue : candidate.whenFalse);
    const present = conditional && unwrap(nullFirst ? candidate.whenFalse : candidate.whenTrue);
    let entry = present && ts.isArrayLiteralExpression(present) && present.elements.length === 1
      && unwrap(present.elements[0]);
    // TS can prove a fresh literal has two readonly slots. Other assertions
    // are not evidence for the shape or origin of a business value.
    if (entry && ts.isAsExpression(entry) && entry.type.getText() === 'const') entry = unwrap(entry.expression);
    if ((!nullFirst && operator !== ts.SyntaxKind.ExclamationEqualsEqualsToken)
        || !empty || !ts.isArrayLiteralExpression(empty) || empty.elements.length
        || !entry || !ts.isArrayLiteralExpression(entry) || entry.elements.length !== 2
        || !ts.isStringLiteral(entry.elements[0])) {
      fail('readonly Map requires explicitly null-guarded literal text entries');
    }
    const operands = [unwrap(cond.left), unwrap(cond.right)];
    const nullIndex = operands.findIndex((operand) => operand.kind === ts.SyntaxKind.NullKeyword);
    const subject = operands[1 - nullIndex];
    const [key, raw] = entry.elements;
    const value = unwrap(raw);
    const input = ts.isIdentifier(value) ? names.get(value.text) : undefined;
    if (!subject || !ts.isIdentifier(subject) || !input || value.text !== subject.text || input.type !== 'text?')
      fail('readonly Map requires each null guard to protect the inserted value');
    if (seen.has(key.text)) fail('readonly Map requires unique literal keys');
    seen.add(key.text);
    return {key: key.text, input: input.name};
  });
}

const numericOperators = {
  '+': ['number', 'add'],
  '>': ['bool', 'gt'], '<': ['bool', 'gt', true],
  '>=': ['bool', 'gt', true, true], '<=': ['bool', 'gt', false, true],
  '===': ['bool', 'eq'], '!==': ['bool', 'eq', false, true],
};

function compile(source, file, entry, physicalPath) {
  const {sf, fn} = typedModuleAst(source, physicalPath, entry);
  const formals = namedParameters(fn.parameters);
  if (formals.error) fail(formals.error);
  const isListener = fn.type?.kind === ts.SyntaxKind.VoidKeyword;
  const isTuple = sf.statements.some(node => ts.isTypeAliasDeclaration(node) && ts.isTupleTypeNode(node.type));
  if (isListener || isTuple) {
    const checker = checkedTypeScript(physicalPath, sf, fail);
    if (fn.parameters.some(p => p.type?.kind !== ts.SyntaxKind.BooleanKeyword)) {
      fail('this typed statement adapter requires explicit Boolean entry parameters');
    }
    const shape = isListener ? compileTypedListener(checker, sf, fn, file)
      : compileTypedTuple(checker, sf, fn);
    const params = shape.params.map((name, position) => ({publicName: name, name, position, domain: 'boolean'}));
    const returns = isListener ? 'void' : 'number';
    return projectInput(file, entry, params, returns, false, returns, shape.result, shape.excerpt, shape);
  }
  const compiled = compileScalar(sf, fn, file, entry, formals.names);
  checkedTypeScript(physicalPath, sf, fail);
  return compiled;
}

function compileScalar(sf, fn, file, entry, formals) {
  // Imports, executable top-level initialization and other module state
  // cannot be ignored by a closed standalone scalar adapter.
  if (sf.statements.some(node => !ts.isFunctionDeclaration(node) &&
      !ts.isTypeAliasDeclaration(node) && !ts.isInterfaceDeclaration(node))) {
    fail('standalone scalar modules require closed function declarations');
  }
  if (fn.asteriskToken) fail('generator entry requires a closed completion contract: ' + entry);
  const call = leadingParameterCall(fn, formals);
  if (call) {
    return { closure: { schema: 'normalizer.standalone-closure/1', file, entry, ...call } };
  }
  if (fn.modifiers.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) {
    fail('async entry requires a closed completion contract: ' + entry);
  }
  const declaredReturn = fn.type;
  const orderedMap = declaredOrderedMap(declaredReturn);
  const returnDomain = orderedMap ? readonlyMap : declaredParameter(declaredReturn, sf);
  if (!orderedMap && (!parameter(returnDomain) || returnDomain === nullableText)) fail('expected declared business scalar or non-null readonly Vector return type');
  if (orderedMap && sf.statements.some(declaration => declaration.name?.text === 'Map')) {
    fail('readonly Map requires an unshadowed built-in Map');
  }
  const names = new Map();
  const params = fn.parameters.map((param, position) => {
    const domain = declaredParameter(param.type, sf);
    if (!parameter(domain) || (!orderedMap && domain === nullableText)) fail('expected declared business scalar or non-null readonly Vector for ' + param.name.text);
    if (orderedMap && param.name.text === 'Map') fail('readonly Map requires an unshadowed built-in Map');
    const binding = param.name.text;
    if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(binding)) fail('unsupported business parameter name ' + binding);
    names.set(param.name.text, {name: binding, type: parameter(domain)});
    return {publicName: param.name.text, name: binding, domain, position};
  });
  function expression(expr) {
    if (expr.op === 'var') {
      const binding = names.get(expr.name);
      if (!binding) fail('unbound scalar value ' + expr.name);
      return {type: binding.type, node: internal('var', {name: binding.name})};
    }
    if (expr.op === 'literal' && (expr.type === 'boolean' || expr.type === 'number' && Number.isSafeInteger(expr.value))) {
      const type = scalar(expr.type);
      return {type, node: internal('const', {type, value: expr.value})};
    }
    if (expr.op === 'signed-number' && Number.isSafeInteger(expr.value)) {
      return {type: 'number', node: internal('const', {type: 'number', value: expr.value})};
    }
    if (expr.op === 'boolean-not') {
      const operand = expression(expr.operand);
      if (operand.type !== 'bool') fail('logical negation in a standalone value requires a Boolean');
      return {type: 'bool', node: internal('not', {value: operand.node})};
    }
    if (expr.op === 'binary' && Object.hasOwn(numericOperators, expr.operator)) {
      const [type, operator, swap, negate] = numericOperators[expr.operator];
      const left = expression(expr.left), right = expression(expr.right);
      // Negating an ordered comparison is valid on exact business numbers,
      // not on unrestricted JS values (which may include NaN or coercion).
      if (left.type !== 'number' || right.type !== 'number') fail('scalar arithmetic and comparison require two business numbers');
      const [a, b] = swap ? [right, left] : [left, right];
      const node = internal(operator, {left: a.node, right: b.node});
      return {type, node: negate ? internal('not', {value: node}) : node};
    }
    if (expr.op === 'read') {
      const [name, field] = expr.target;
      const binding = names.get(name);
      if (expr.target.length !== 2 || field !== 'length' || binding?.type !== 'number[]') {
        fail('length requires a direct declared non-null readonly Vector');
      }
      return {type: 'number', node: internal('value.field', {value: internal('var', {name: binding.name}), name: 'length'})};
    }
    if (expr.op === 'array') {
      if (expr.items.some(item => item.op === 'opaque')) fail('readonly Vector literal requires a closed list of numbers');
      const items = expr.items.map(expression);
      if (items.some(item => item.type !== 'number')) fail('readonly Vector literal requires a closed list of numbers');
      return {type: 'number[]', node: internal('value.number_vector', {items: items.map(item => item.node)})};
    }
    if (expr.op === 'select') {
      const test = expression(expr.condition), yes = expression(expr.whenTrue), no = expression(expr.whenFalse);
      if (test.type !== 'bool' || yes.type !== no.type) fail('conditional requires a Boolean and two matching business value types');
      return {type: yes.type, node: internal('if', {cond: test.node, then: yes.node, else: no.node})};
    }
    fail('unsupported standalone scalar operation ' + expr.op);
  }
  const [last, ...remaining] = fn.body.statements;
  if (remaining.length || !last || !ts.isReturnStatement(last) || !last.expression) fail('expected one closed return expression');
  const returns = orderedMap ? 'map<text,text>' : parameter(returnDomain);
  const body = orderedMap
    ? internal('value.ordered_text_map', {entries: mapEntries(last.expression, names, fail)})
    : expression(lowerExpression(last.expression, sf)).node;
  return projectInput(file, entry, params, returnDomain, orderedMap, returns, body, fn.getText(sf));
}

function projectInput(file, entry, params, returnDomain, orderedMap, returns, body, excerpt, typed = {}) {
  const binding = 'Entry.run';
  const moduleFile = 'Module.ts';
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
    ...(typed.exceptions && {exceptions: typed.exceptions}),
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
  process.stderr.write(result.stderr );
  process.exitCode = result.status ;
}

try {
  const [kernel, rootArg, flag, locator] = process.argv.slice(2);
  if (flag !== '--entry') fail('expected --entry path.ts#exportedFunction');
  const {file, name: entry} = entryLocator(locator, fail);
  if (!file.endsWith('.ts') || file.endsWith('.d.ts')) fail('standalone entry must be an executable .ts source file');
  const { declared, name, path } = projectSources(rootArg)(file);
  if (!path.endsWith('.ts') || path.endsWith('.d.ts')) fail('standalone .ts entry must resolve to an executable .ts source file');
  const source = readFileSync(path, 'utf8');
  const {carrier, binding, project, closure, moduleFile} = compile(source, name, entry, declared);
  if (closure) {
    const result = invokeKernel(kernel, ['--validate-standalone-closure'], JSON.stringify(closure));
    publishKernelFailure(result);
  } else {
    const temp = mkdtempSync(join(tmpdir(), 'normalizer-ts-module-'));
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
