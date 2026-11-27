// Standalone TypeScript source, checked and lowered without executing user code.
// It parses, never executes, the module and uses the shared PERS kernel.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { invokeKernel } from './invoke-kernel.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import ts from 'typescript';
import { closedReceiver } from './module-receiver.mjs';
import { checkedProjectModules, hasProjectImports } from './module-links.mjs';
import { definedModuleExport, isLocalExportList, parseSourceModule, lowerExpression, unwrapParens as unwrap } from './ir.mjs';
import { eventContract } from './module-event-project.mjs';
import { compileTypedTuple } from './module-tuple.mjs';
import { compileTypedListener } from './module-listener.mjs';
import { checkedTypeScript, failTypedModule, typedModuleAst } from './typed-module.mjs';
import { entryLocator } from './entry-locator.mjs';
import { guardedUnknownText } from './module-unknown-text.mjs';
import { projectSources } from './project-source.mjs';
import {isOfficialNamespaceImport, standardCalls} from './standard-library.mjs';

const domains = new Map([['number', ['int', 'number']], ['boolean', ['boolean', 'bool']], ['string', ['String', 'text']]]);
const scalar = domain => domains.get(domain)?.[1];
const readonlyVector = 'ReadonlyArray<number>';
const readonlyMap = 'ReadonlyMap<string,string>';
const nullableText = 'string|null';
const parameter = domain => scalar(domain) ?? (domain === readonlyVector ? 'number[]'
  : domain === nullableText ? 'text?' : domain === 'unknown' ? 'unknown' : null);
const carrier = domain => domain === readonlyVector ? 'int[]'
  : domain === nullableText ? 'String' : domain === 'unknown' ? 'Object' : domains.get(domain)[0];
const valueType = domain => domain === readonlyVector ? 'vector<number>'
  : domain === 'string' ? 'text?' : domain === 'unknown' ? 'ts_unknown_input' : parameter(domain);
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
  '+': ['number', 'add'], '-': ['number', 'sub'],
  '>': ['bool', 'gt'], '<': ['bool', 'lt'],
  '>=': ['bool', 'lt', true], '<=': ['bool', 'gt', true],
  '===': ['bool', 'eq'], '!==': ['bool', 'eq', true],
};

function compile(source, file, entry, physicalPath, linked) {
  if (entry.includes('.')) {
    const sf = linked?.ast ?? parseSourceModule(physicalPath, source, fail, ts.ScriptKind.TS);
    const official = !linked && sf.statements.some(isOfficialNamespaceImport);
    const {fn, fields, assembly} = closedReceiver(sf, entry, fail, {linked: Boolean(linked) || official});
    const formals = namedParameters(fn.parameters);
    if (formals.error) fail(formals.error);
    const checker = !linked ? checkedTypeScript(physicalPath, sf, fail, {allowOfficial: official}) : undefined;
    return compileScalar(sf, fn, file, entry, formals.names, {receiverFields: fields, receiverAssembly: assembly, linked, officialChecker: official ? checker : undefined});
  }
  if (linked) {
    const sf = linked.ast;
    const fn = definedModuleExport(sf, entry, fail);
    const formals = namedParameters(fn.parameters);
    if (formals.error) fail(formals.error);
    return compileScalar(sf, fn, file, entry, formals.names, {linked});
  }
  const {sf, fn} = typedModuleAst(source, physicalPath, entry);
  const formals = namedParameters(fn.parameters);
  if (formals.error) fail(formals.error);
  const isListener = fn.type?.kind === ts.SyntaxKind.VoidKeyword;
  const isStateEvent = isListener && fn.body.statements.some(st => ts.isExpressionStatement(st)
    && ts.isBinaryExpression(st.expression) && st.expression.operatorToken.kind === ts.SyntaxKind.EqualsToken
    && ts.isPropertyAccessExpression(st.expression.left));
  if (isStateEvent) {
    const project = eventContract(file, source, entry, physicalPath, {synchronous: true});
    return {carrier: project.source_text, carrierFile: project.source, binding: project.entries[0], project, moduleFile: 'Prepared.ts'};
  }
  const usesStandard = sf.statements.some(isOfficialNamespaceImport);
  const checker = checkedTypeScript(physicalPath, sf, fail, {allowOfficial: usesStandard});
  if (fn.parameters.some(param => checker.getTypeAtLocation(param.name).flags & ts.TypeFlags.Unknown)) {
    const [statement] = fn.body.statements;
    const [parameter] = fn.parameters;
    const simple = fn.parameters.length === 1 && fn.body.statements.length === 1
      && (ts.isThrowStatement(statement) || ts.isReturnStatement(statement))
      && statement.expression && ts.isIdentifier(unwrap(statement.expression))
      && !fn.asteriskToken && !fn.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.AsyncKeyword)
      && checker.getSymbolAtLocation(unwrap(statement.expression)) === checker.getSymbolAtLocation(parameter.name)
      && sf.statements.every(declaration => declaration === fn
        || ts.isTypeAliasDeclaration(declaration) || ts.isInterfaceDeclaration(declaration)
        || isLocalExportList(declaration));
    if (simple) {
      const name = parameter.name.text;
      const params = [{publicName: name, name, position: 0, domain: 'unknown'}];
      const signature = checker.getSignatureFromDeclaration(fn);
      const returnedType = signature && checker.getReturnTypeOfSignature(signature);
      if (ts.isThrowStatement(statement) && (returnedType?.flags & ts.TypeFlags.Void)) {
        return projectInput(file, entry, params, 'void', false, 'void',
          {op: 'source.throw_unknown_input', name}, fn.getText(sf));
      }
      if (ts.isReturnStatement(statement) && (returnedType?.flags & ts.TypeFlags.Unknown)) {
        return projectInput(file, entry, params, 'unknown', false, 'unknown',
          {op: 'var', name}, fn.getText(sf));
      }
    }
    const proof = guardedUnknownText(sf, fn, checker, fail);
    const params = [{publicName: proof.name, name: proof.name, position: 0, domain: 'unknown'}];
    return projectInput(file, entry, params, 'string', false, 'text',
      {op: 'source.guard_unknown_text', ...proof}, fn.getText(sf), {exceptions: {TypeError: 'text'}});
  }
  const isTuple = sf.statements.some(node => ts.isTypeAliasDeclaration(node) && ts.isTupleTypeNode(node.type));
  if (isListener || isTuple) {
    if (usesStandard) fail('typed listener and tuple modules do not use the official text namespace');
    if (fn.parameters.some(p => p.type?.kind !== ts.SyntaxKind.BooleanKeyword)) {
      fail('this typed statement adapter requires explicit Boolean entry parameters');
    }
    const shape = isListener ? compileTypedListener(checker, sf, fn, file)
      : compileTypedTuple(checker, sf, fn);
    const params = shape.params.map((name, position) => ({publicName: name, name, position, domain: 'boolean'}));
    const returns = isListener ? 'void' : 'number';
    return projectInput(file, entry, params, returns, false, returns, shape.result, shape.excerpt, shape);
  }
  return compileScalar(sf, fn, file, entry, formals.names, {officialChecker: usesStandard ? checker : undefined});
}

function compileScalar(sf, fn, file, entry, formals, {receiverFields, receiverAssembly = [], linked, officialChecker} = {}) {
  // Nonstandard imports, top-level initialization and other module state
  // require a separate certified linked graph or receiver assembly.
  if (!receiverFields && !linked && sf.statements.some(node => !ts.isFunctionDeclaration(node) &&
      !ts.isTypeAliasDeclaration(node) && !ts.isInterfaceDeclaration(node) && !isOfficialNamespaceImport(node)
      && !isLocalExportList(node))) {
    fail('standalone scalar modules require closed function declarations');
  }
  if (fn.asteriskToken) fail('generator entry requires a closed completion contract: ' + entry);
  const call = leadingParameterCall(fn, formals);
  if (call) {
    return { closure: { schema: 'normalizer.standalone-closure/1', file, entry, ...call } };
  }
  if (fn.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) {
    fail('async entry requires a closed completion contract: ' + entry);
  }
  const declaredReturn = fn.type;
  const orderedMap = declaredOrderedMap(declaredReturn);
  const returnDomain = orderedMap ? readonlyMap : declaredParameter(declaredReturn, sf);
  if (!orderedMap && (!parameter(returnDomain) || returnDomain === 'unknown')) fail('expected declared business scalar or non-null readonly Vector return type');
  if (orderedMap && sf.statements.some(declaration => declaration.name?.text === 'Map')) {
    fail('readonly Map requires an unshadowed built-in Map');
  }
  const names = new Map();
  const params = fn.parameters.map((param, position) => {
    const domain = declaredParameter(param.type, sf);
    if (!parameter(domain) || domain === 'unknown') fail('expected declared business scalar or non-null readonly Vector for ' + param.name.text);
    if (orderedMap && param.name.text === 'Map') fail('readonly Map requires an unshadowed built-in Map');
    const binding = param.name.text;
    if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(binding)) fail('unsupported business parameter name ' + binding);
    names.set(param.name.text, {name: binding, type: parameter(domain)});
    return {publicName: param.name.text, name: binding, domain, position};
  });
  const active = new Set([fn]);
  let linkedCallBudget = 8_192;
  const checker = officialChecker ?? (linked?.hasOfficial ? linked.checker : undefined);
  const lowerOfficial = checker && standardCalls(checker, fail);
  const lowerCall = (node, lower) => {
    const official = lowerOfficial?.(node, lower);
    if (official !== undefined) return official;
    if (!linked || !ts.isCallExpression(node)) return;
    const bound = !node.questionDotToken && !node.typeArguments?.length && linked.helper(unwrap(node.expression));
    if (!bound) fail('linked call has no closed project function: ' + node.expression.getText());
    return {op: 'linked-call', ...bound, args: node.arguments.map(lower)};
  };
  // Text literals are admitted for closed text expressions, not as vector slots.
  const textExpressions = checker || returnDomain === 'string' || returnDomain === nullableText;
  // Lower all module-startup expressions, including fields unused by the
  // selected method. The linked scalar rules admit only closed, pure
  // computations here, so reading their acquired value may reuse that IR.
  const initializerValues = new Map();
  for (const initializer of receiverAssembly) {
    const value = expression(lowerExpression(initializer, sf, lowerCall), new Map());
    if (!['number', 'bool', 'text', 'text?'].includes(value.type)) fail('object receiver requires a closed pure scalar initializer');
    initializerValues.set(initializer, value);
  }
  function expression(expr, lexical = names) {
    const evaluate = child => expression(child, lexical);
    if (expr.op === 'linked-call') {
      const {declaration, origin} = expr;
      const label = origin.name + '#' + declaration.name?.text;
      if (active.has(declaration)) fail('recursive linked function is not a closed scalar helper: ' + label);
      if (--linkedCallBudget < 0 || active.size >= 128) {
        throw new Error('[NORM-ANALYSIS-INCOMPLETE] linked helper expansion budget exceeded at ' + label);
      }
      if (declaration.asteriskToken || declaration.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) {
        fail('linked helper has no synchronous completion contract: ' + label);
      }
      const formals = namedParameters(declaration.parameters);
      if (formals.error || declaration.parameters.length !== expr.args.length) fail('linked helper has wrong arity: ' + label);
      const scope = new Map();
      for (const [i, param] of declaration.parameters.entries()) {
        const expected = parameter(declaredParameter(param.type, origin.ast));
        const actual = evaluate(expr.args[i]);
        if (!expected || actual.type !== expected) fail('linked helper argument is outside the declared value domain: ' + label);
        scope.set(param.name.text, actual);
      }
      const [statement, ...rest] = declaration.body.statements;
      if (rest.length || !statement || !ts.isReturnStatement(statement) || !statement.expression) {
        fail('linked helper requires one closed pure return expression: ' + label);
      }
      active.add(declaration);
      const result = expression(lowerExpression(statement.expression, origin.ast, lowerCall), scope);
      active.delete(declaration);
      const returned = parameter(declaredParameter(declaration.type, origin.ast));
      if (!returned || result.type !== returned) fail('linked helper result is outside the declared value domain: ' + label);
      return result;
    }
    if (expr.op === 'var') {
      const binding = lexical.get(expr.name);
      if (!binding) fail('unbound scalar value ' + expr.name);
      return binding.node ? binding : {type: binding.type, node: internal('var', {name: binding.name})};
    }
    if (expr.op === 'literal' && (expr.type === 'boolean' || expr.type === 'number')) {
      const type = scalar(expr.type);
      return {type, node: internal('const', {type, value: expr.value})};
    }
    if (textExpressions && expr.op === 'literal' && expr.type === 'string')
      return {type: 'text', node: internal('const', {type: 'text?', value: expr.value})};
    if (textExpressions && expr.op === 'literal' && expr.type === 'null')
      return {type: 'text?', node: internal('const', {type: 'unit', value: null})};
    if (expr.op === 'official-call') {
      if (expr.name === 'currentTimeMillis' && expr.args.length === 0) {
        // Link-time expression substitution can duplicate or erase a helper
        // argument. Don't lower observations through that pure path.
        if (linked) fail('linked clock call requires source-ordered acquisition before helper substitution');
        return {type: 'number', node: internal('source.clock')};
      }
      const args = expr.args.map(evaluate);
      if (expr.name === 'trim' && args.length === 1 && ['text', 'text?'].includes(args[0].type))
        return {type: args[0].type, node: internal('value.text_trim', {value: args[0].node})};
      if (expr.name === 'defaultIfBlank' && args.length === 2 && ['text', 'text?'].includes(args[0].type)
          && args[1].type === 'text')
        return {type: 'text', node: internal('value.text_default_if_blank', {value: args[0].node, fallback: args[1].node})};
      fail('unsupported official @normalizer/stdlib text signature');
    }
    if (expr.op === 'boolean-not') {
      const operand = evaluate(expr.operand);
      if (operand.type !== 'bool') fail('logical negation in a standalone value requires a Boolean');
      return {type: 'bool', node: internal('not', {value: operand.node})};
    }
    if (expr.op === 'binary' && (expr.operator === '&&' || expr.operator === '||')) {
      const left = evaluate(expr.left), right = evaluate(expr.right);
      if (left.type !== 'bool' || right.type !== 'bool') fail('short circuit needs two declared Boolean values');
      const and = expr.operator === '&&';
      const literal = internal('const', {type: 'bool', value: !and});
      return {type: 'bool', node: internal('if', {
        cond: left.node, then: and ? right.node : literal, else: and ? literal : right.node,
      })};
    }
    if (expr.op === 'binary' && Object.hasOwn(numericOperators, expr.operator)) {
      const [type, operator, negate] = numericOperators[expr.operator];
      const left = evaluate(expr.left), right = evaluate(expr.right);
      // Negating an ordered comparison is valid on exact business numbers,
      // not on unrestricted JS values (which may include NaN or coercion).
      const numberPair = left.type === 'number' && right.type === 'number';
      const booleanEquality = operator === 'eq' && left.type === 'bool' && right.type === 'bool';
      if (!numberPair && !booleanEquality) fail('scalar operations need two numbers or, for equality, two Booleans');
      // Preserve source evaluation order; the kernel canonicalizes only
      // the resulting already acquired operands.
      const node = internal(operator, {left: left.node, right: right.node});
      return {type, node: negate ? internal('not', {value: node}) : node};
    }
    if (expr.op === 'read') {
      const [name, field] = expr.target;
      if (name === 'this' && receiverFields && lexical === names) {
        const initializer = expr.target.length === 2 && receiverFields.get(field);
        const value = initializer && initializerValues.get(initializer);
        if (!value) fail('entry receiver field is not a proved acquired scalar: ' + field);
        return value;
      }
      const binding = lexical.get(name);
      if (expr.target.length !== 2 || field !== 'length' || binding?.type !== 'number[]') {
        fail('length requires a direct declared non-null readonly Vector');
      }
      return {type: 'number', node: internal('value.field', {value: binding.node ?? internal('var', {name: binding.name}), name: 'length'})};
    }
    if (expr.op === 'array') {
      if (expr.items.some(item => item.op === 'opaque')) fail('readonly Vector literal requires a closed list of numbers');
      const items = expr.items.map(evaluate);
      if (items.some(item => item.type !== 'number')) fail('readonly Vector literal requires a closed list of numbers');
      return {type: 'number[]', node: internal('value.number_vector', {items: items.map(item => item.node)})};
    }
    if (expr.op === 'select') {
      const test = evaluate(expr.condition), yes = evaluate(expr.whenTrue), no = evaluate(expr.whenFalse);
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
    : expression(lowerExpression(last.expression, sf, lowerCall)).node;
  if (linked) {
    // Substitution can reuse an expression at many call sites. Count
    // occurrences, not object identities, before JSON expands the IR.
    const outstanding = [body];
    let remainingNodes = 65_536;
    while (outstanding.length) {
      const next = outstanding.pop();
      if (--remainingNodes < 0) {
        throw new Error('[NORM-ANALYSIS-INCOMPLETE] linked result size budget exceeded at ' + file + '#' + entry);
      }
      if (next !== null && typeof next === 'object') {
        for (const value of Object.values(next)) outstanding.push(value);
      }
    }
  }
  return projectInput(file, entry, params, returnDomain, orderedMap, returns, body, fn.getText(sf));
}

function projectInput(file, entry, params, returnDomain, orderedMap, returns, body, excerpt, typed = {}) {
  const binding = 'Entry.run';
  const moduleFile = 'Module.ts';
  const javaParams = params.map(param => carrier(param.domain));
  const declarations = javaParams.map((type, index) => type + ' ' + params[index].name);
  const resultTypes = returnDomain === readonlyVector ? {ReadonlyNumbers: 'vector<number>'}
    : returnDomain === 'unknown' ? {OpenSourceValue: 'ts_unknown_input'}
    : orderedMap ? {ReadonlyLabels: returns} : {};
  const returnCarrier = returnDomain === 'void' ? 'void' : Object.keys(resultTypes)[0] ?? carrier(returnDomain);
  const javaSource = Object.keys(resultTypes).map(name => 'class ' + name + ' {}\n').join('') +
    'class Entry { static native ' + returnCarrier + ' run(' + declarations.join(', ') + '); }\n';
  return {carrier: javaSource, binding, moduleFile, project: {
    schema: 'normalizer.kernel-input/1', source: 'Carrier.java', source_text: javaSource, entries: [binding],
    result_value_types: resultTypes,
    ...(typed.ports && {ports: typed.ports}),
    ...(typed.exceptions && {exceptions: typed.exceptions}),
    parameter_value_types: {run: Object.fromEntries(params.filter(p => p.domain === readonlyVector || p.domain === 'unknown')
        .map(p => [p.name, valueType(p.domain)]))},
    entry_contexts: {[binding]: {
      function: file + '#' + entry, returns,
      ...(typed.externalResults && {external_results: typed.externalResults}),
      ...(typed.environment && {environment: typed.environment}),
      parameters: Object.fromEntries(params.map(param => [param.publicName, {position: param.position, type: parameter(param.domain)}])),
    }},
    functions: {run: {
      java: binding + '(' + javaParams.join(',') + ')',
      params: params.map(param => ({name: param.name, type: valueType(param.domain)})),
      returns: returns === 'void' ? 'unit' : resultTypes[returnCarrier] ?? valueType(returnDomain), origin: {path: moduleFile, excerpt},
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
  const resolveProjectSource = projectSources(rootArg);
  const sourceEntry = resolveProjectSource(file);
  const { declared, name, path } = sourceEntry;
  if (!path.endsWith('.ts') || path.endsWith('.d.ts')) fail('standalone .ts entry must resolve to an executable .ts source file');
  const source = readFileSync(path, 'utf8');
  const rootReceiver = entry.includes('.');
  const linked = rootReceiver || hasProjectImports(source, declared)
    ? checkedProjectModules(sourceEntry, resolveProjectSource, fail, {rootReceiver}) : undefined;
  const {carrier, carrierFile = 'Carrier.java', binding, project, closure, moduleFile} = compile(source, name, entry, declared, linked);
  if (closure) {
    const result = invokeKernel(kernel, ['--validate-standalone-closure'], JSON.stringify(closure));
    publishKernelFailure(result);
  } else {
    const temp = mkdtempSync(join(tmpdir(), 'normalizer-ts-module-'));
    try {
      writeFileSync(join(temp, carrierFile), carrier);
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
