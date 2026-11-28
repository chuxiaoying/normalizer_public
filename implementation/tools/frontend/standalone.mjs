// Standalone TypeScript source, checked and lowered without executing user code.
// It parses, never executes, the module and uses the shared PERS kernel.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { invokeKernel } from './invoke-kernel.mjs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import ts from 'typescript';
import { closedReceiver } from './module-receiver.mjs';
import { checkedProjectModules, hasProjectImports } from './module-links.mjs';
import { definedModuleExport, isLocalExportList, parseSourceModule, lowerExpression, textWireValue, unwrapParens as unwrap } from './ir.mjs';
import { eventContract } from './module-event-project.mjs';
import { compileTypedTuple } from './module-tuple.mjs';
import { compileTypedListener } from './module-listener.mjs';
import { checkedTypeScript, failTypedModule, isCompilerLibrary, typedModuleAst } from './typed-module.mjs';
import { entryLocator } from './entry-locator.mjs';
import { nativeTextFlow, unknownValueModule } from './module-unknown-text.mjs';
import { projectSources } from './project-source.mjs';
import {isOfficialNamespaceImport, standardCalls} from './standard-library.mjs';

const domains = new Map([['number', ['int', 'number']], ['boolean', ['boolean', 'bool']], ['string', ['String', 'text']]]);
const scalar = domain => domains.get(domain)?.[1];
const readonlyVector = 'ReadonlyArray<number>';
const readonlyMap = 'ReadonlyMap<string,string>';
const nullableText = 'string|null';
const isTextDomain = kind => kind === 'text' || kind === 'text?';
const parameter = domain => scalar(domain) ?? (domain === readonlyVector ? 'number[]'
  : domain === nullableText ? 'text?' : domain === 'unknown' ? 'unknown' : null);
const carrier = domain => domain === readonlyVector ? 'int[]'
  : domain === nullableText ? 'String' : domain === 'unknown' ? 'Object' : domains.get(domain)[0];
const valueType = domain => domain === readonlyVector ? 'vector<number>'
  : domain === 'string' ? 'text?' : domain === 'unknown' ? 'ts_unknown_input' : parameter(domain);
function declaredParameter(node, checker) {
  return resolvedParameter(node && checker?.getTypeAtLocation(node), checker);
}

function resolvedParameter(type, checker) {
  // Compare the compiler's native array type, not a source identifier
  // named ReadonlyArray. A tuple or branded primitive is not a vector.
  if (type && checker.isArrayType(type) && type.target?.symbol?.name === 'ReadonlyArray') {
    const [item, ...extra] = checker.getTypeArguments(type);
    if (item?.flags === ts.TypeFlags.Number && !extra.length) return readonlyVector;
  }
  if (type?.isUnion() && type.types.length === 2
      && type.types.some(member => member.flags === ts.TypeFlags.String)
      && type.types.some(member => member.flags === ts.TypeFlags.Null)) return nullableText;
  // Use only resolved primitive domains. Do not fall back to the
  // source spelling: it could match an internal carrier string even when
  // the compiler resolved a different declaration.
  if (type?.flags === ts.TypeFlags.Number) return 'number';
  if (type?.flags === ts.TypeFlags.String) return 'string';
  if (type && (type.flags & ts.TypeFlags.Boolean)) return 'boolean';
}

function declaredRecord(node, checker) {
  if (!node || !checker || !ts.isTypeReferenceNode(node)) return;
  const type = checker.getTypeAtLocation(node);
  if (!(type.flags & ts.TypeFlags.Object)) return;
  const symbol = type.aliasSymbol ?? type.getSymbol();
  if (symbol?.declarations?.length !== 1) return;
  const [declaration] = symbol.declarations;
  if (declaration.getSourceFile().isDeclarationFile) return;
  const members = ts.isInterfaceDeclaration(declaration) && !declaration.heritageClauses?.length
    ? declaration.members
    : ts.isTypeAliasDeclaration(declaration) && ts.isTypeLiteralNode(declaration.type)
      ? declaration.type.members : undefined;
  const name = symbol.name;
  if (!members?.length || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)
      || ['void','unknown','number','money','date','instant','bool','text','bytes'].includes(name)) return;
  const fields = new Map();
  for (const member of members) {
    if (!ts.isPropertySignature(member) || member.questionToken || !ts.isIdentifier(member.name)) return;
    const key = member.name.text;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) || fields.has(key)) return;
    const property = checker.getPropertyOfType(type, key);
    const domain = property && resolvedParameter(checker.getTypeOfSymbolAtLocation(property, node), checker);
    if (!['number', 'boolean', 'string', nullableText].includes(domain)) return;
    fields.set(key, domain);
  }
  const wire = 'record<' + Array.from(fields, ([key, domain]) => key + ':' + valueType(domain)).sort().join(',') + '>';
  return {name, declaration, fields, wire};
}

const compilerLibrary = dirname(ts.getDefaultLibFilePath({}));
function declaredOrderedMap(node, checker) {
  if (!node || !checker) return false;
  const type = checker.getTypeAtLocation(node);
  const symbol = type.target?.symbol;
  if (symbol?.name !== 'ReadonlyMap' || !symbol.declarations?.length
      || !symbol.declarations.every(declaration =>
        isCompilerLibrary(declaration.getSourceFile().fileName, compilerLibrary))) {
    return false;
  }
  const [key, value, ...extra] = checker.getTypeArguments(type);
  return !extra.length && key?.flags === ts.TypeFlags.String && value?.flags === ts.TypeFlags.String;
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
  const textPair = node => {
    let entry = unwrap(node);
    // A const assertion preserves the concrete tuple shape. Other casts
    // are not proof of a value's shape, type or acquisition.
    if (ts.isAsExpression(entry) && entry.type.getText() === 'const') entry = unwrap(entry.expression);
    if (!ts.isArrayLiteralExpression(entry) || entry.elements.length !== 2
        || !ts.isStringLiteral(entry.elements[0])) return;
    return [textWireValue(entry.elements[0].text), unwrap(entry.elements[1])];
  };
  return argument.elements.map(row => {
    if (!ts.isSpreadElement(row)) {
      const pair = textPair(row);
      if (!pair || !ts.isStringLiteral(pair[1])) fail('readonly Map unconditional entries must contain literal text');
      const [key, text] = pair;
      return {key, source: {kind: 'literal', text: textWireValue(text.text)}};
    }
    const candidate = unwrap(row.expression);
    const conditional = candidate && ts.isConditionalExpression(candidate);
    const cond = conditional && unwrap(candidate.condition);
    const operator = cond && ts.isBinaryExpression(cond) && cond.operatorToken.kind;
    const nullFirst = operator === ts.SyntaxKind.EqualsEqualsEqualsToken;
    const empty = conditional && unwrap(nullFirst ? candidate.whenTrue : candidate.whenFalse);
    const present = conditional && unwrap(nullFirst ? candidate.whenFalse : candidate.whenTrue);
    const entry = present && ts.isArrayLiteralExpression(present) && present.elements.length === 1
      && textPair(present.elements[0]);
    if ((!nullFirst && operator !== ts.SyntaxKind.ExclamationEqualsEqualsToken)
        || !empty || !ts.isArrayLiteralExpression(empty) || empty.elements.length || !entry) {
      fail('readonly Map requires explicitly null-guarded literal text entries');
    }
    const operands = [unwrap(cond.left), unwrap(cond.right)];
    const nullIndex = operands.findIndex((operand) => operand.kind === ts.SyntaxKind.NullKeyword);
    const subject = operands[1 - nullIndex];
    const [key, value] = entry;
    const input = ts.isIdentifier(value) ? names.get(value.text) : undefined;
    if (!subject || !ts.isIdentifier(subject) || !input || value.text !== subject.text || input.type !== 'text?')
      fail('readonly Map requires each null guard to protect the inserted value');
    return {key, source: {kind: 'parameter', name: input.name}};
  });
}

const numericOperators = {
  '+': ['number', 'add'], '-': ['number', 'sub'], '*': ['number', 'mul'],
  '>': ['bool', 'gt'], '<': ['bool', 'lt'],
  '>=': ['bool', 'lt', true], '<=': ['bool', 'gt', true],
  '===': ['bool', 'eq'], '!==': ['bool', 'eq', true],
};

function compileUnknownValue(sf, fn, file, entry, checker, allowTypeImports = false) {
  if (!fn.parameters.some(param => checker.getTypeAtLocation(param.name).flags & ts.TypeFlags.Unknown)) return;
  const [statement] = fn.body.statements;
  const [parameter] = fn.parameters;
  const simple = fn.parameters.length === 1 && fn.body.statements.length === 1
    && (ts.isThrowStatement(statement) || ts.isReturnStatement(statement))
    && statement.expression && ts.isIdentifier(unwrap(statement.expression))
    && !fn.asteriskToken && !fn.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.AsyncKeyword)
    && checker.getSymbolAtLocation(unwrap(statement.expression)) === checker.getSymbolAtLocation(parameter.name)
    && unknownValueModule(sf, fn, allowTypeImports);
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
  const general = nativeTextFlow(sf, fn, checker, file + '#' + entry, allowTypeImports);
  if (general) {
    return projectInput(file, entry, general.params, 'string', false, 'text',
      {op: 'source.native_text_flow', flow: general.flow}, fn.getText(sf), {exceptions: general.exceptions});
  }
  const first = fn.body.statements[0];
  if (!first || !ts.isIfStatement(first))
    fail('returning text from an unknown input requires a native type decision');
  fail('native text flow requires strict native comparisons and a proved text result on every normal path');
}

function compile(source, file, entry, physicalPath, linked) {
  if (entry.includes('.')) {
    const sf = linked?.ast ?? parseSourceModule(physicalPath, source, fail, ts.ScriptKind.TS);
    const official = !linked && sf.statements.some(isOfficialNamespaceImport);
    const {fn, fields, assembly} = closedReceiver(sf, entry, fail, {linked: Boolean(linked) || official});
    const formals = namedParameters(fn.parameters);
    if (formals.error) fail(formals.error);
    const checker = !linked ? checkedTypeScript(physicalPath, sf, fail, {allowOfficial: official}) : undefined;
    return compileScalar(sf, fn, file, entry, formals.names, {receiverFields: fields, receiverAssembly: assembly, linked, typeChecker: checker, officialChecker: official ? checker : undefined});
  }
  if (linked) {
    const sf = linked.ast;
    const fn = definedModuleExport(sf, entry, fail);
    const formals = namedParameters(fn.parameters);
    if (formals.error) fail(formals.error);
    return compileUnknownValue(sf, fn, file, entry, linked.checker, true)
      ?? compileScalar(sf, fn, file, entry, formals.names, {linked});
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
  const unknown = compileUnknownValue(sf, fn, file, entry, checker);
  if (unknown) return unknown;
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
  return compileScalar(sf, fn, file, entry, formals.names, {typeChecker: checker, officialChecker: usesStandard ? checker : undefined});
}

function compileScalar(sf, fn, file, entry, formals, {receiverFields, receiverAssembly = [], linked, typeChecker, officialChecker} = {}) {
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
  const bindings = linked?.checker ?? typeChecker;
  const declaredReturn = fn.type;
  const orderedMap = declaredOrderedMap(declaredReturn, bindings);
  const returnDomain = orderedMap ? readonlyMap : declaredParameter(declaredReturn, bindings);
  if (!orderedMap && (!parameter(returnDomain) || returnDomain === 'unknown')) fail('expected declared business scalar or non-null readonly Vector return type');
  if (orderedMap && sf.statements.some(declaration => declaration.name?.text === 'Map')) {
    fail('readonly Map requires an unshadowed built-in Map');
  }
  const names = new Map();
  const boundRecords = new Map();
  const params = fn.parameters.map((param, position) => {
    const domain = declaredParameter(param.type, bindings);
    const record = parameter(domain) ? undefined : declaredRecord(param.type, bindings);
    if ((!parameter(domain) || domain === 'unknown') && !record)
      fail('expected declared business scalar or non-null readonly Vector for ' + param.name.text);
    if (orderedMap && param.name.text === 'Map') fail('readonly Map requires an unshadowed built-in Map');
    const binding = param.name.text;
    if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(binding)) fail('unsupported business parameter name ' + binding);
    if (record) {
      const existing = boundRecords.get(record.name);
      if (existing && (existing.declaration !== record.declaration || existing.wire !== record.wire))
        fail('same public record name resolves to different declarations or field domains: ' + record.name);
      boundRecords.set(record.name, record);
    }
    names.set(param.name.text, {name: binding, type: record ? record.name : parameter(domain), record});
    return {publicName: param.name.text, name: binding, domain, position, record};
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
  // Lower all module-startup expressions, including fields unused by the
  // selected method. The linked scalar rules admit only closed, pure
  // computations here, so reading their acquired value may reuse that IR.
  const initializerValues = new Map();
  for (const initializer of receiverAssembly) {
    const value = expression(lowerExpression(initializer, sf, lowerCall), new Map());
    if (!['number', 'bool', 'text', 'text?'].includes(value.type)) fail('object receiver requires a closed pure scalar initializer');
    initializerValues.set(initializer, value);
  }
  function expression(expr, lexical = names, entryScope = lexical === names) {
    const evaluate = child => expression(child, lexical, entryScope);
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
        const expected = parameter(declaredParameter(param.type, bindings));
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
      const returned = parameter(declaredParameter(declaration.type, bindings));
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
    if (expr.op === 'literal' && expr.type === 'string')
      return {type: 'text', node: internal('const', {type: 'text?', value: expr.value})};
    if (expr.op === 'literal' && expr.type === 'null')
      return {type: 'text?', node: internal('const', {type: 'text?', value: null})};
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
      // Native text concatenation uses the two acquired values in source
      // order. The exact-number path below never handles coercion.
      if (operator === 'add' && left.type === 'text' && right.type === 'text') {
        return {type: 'text', node: internal('value.text_concat', {left: left.node, right: right.node})};
      }
      // Negating an ordered comparison is valid on exact business numbers,
      // not on unrestricted JS values (which may include NaN or coercion).
      const numberPair = left.type === 'number' && right.type === 'number';
      const booleanEquality = operator === 'eq' && left.type === 'bool' && right.type === 'bool';
      const textEquality = operator === 'eq' && [left.type, right.type].every(isTextDomain);
      if (!numberPair && !booleanEquality && !textEquality) {
        fail('scalar operations need two numbers or, for equality, two Booleans or two text/null values');
      }
      // Native JS primitive text equality does not share Java object-identity
      // semantics. Its operands are still acquired in source order.
      const node = internal(textEquality ? 'value.text_equal' : operator, {left: left.node, right: right.node});
      return {type, node: negate ? internal('not', {value: node}) : node};
    }
    if (expr.op === 'read') {
      const [name, field] = expr.target;
      if (name === 'this' && receiverFields && entryScope) {
        const initializer = expr.target.length === 2 && receiverFields.get(field);
        const value = initializer && initializerValues.get(initializer);
        if (!value) fail('entry receiver field is not a proved acquired scalar: ' + field);
        return value;
      }
      const binding = lexical.get(name);
      if (expr.target.length === 2 && binding?.record?.fields.has(field)) {
        const domain = binding.record.fields.get(field);
        return {type: parameter(domain), node: internal('value.field', {
          value: binding.node ?? internal('var', {name: binding.name}), name: field,
        })};
      }
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
      const test = evaluate(expr.condition);
      let yesScope = lexical, noScope = lexical;
      const condition = expr.condition;
      if (condition.op === 'binary' && ['===', '!=='].includes(condition.operator)) {
        const isNull = node => node.op === 'literal' && node.type === 'null';
        const other = isNull(condition.left) ? condition.right : isNull(condition.right) ? condition.left : undefined;
        const binding = other?.op === 'var' && lexical.get(other.name);
        if (binding?.type === 'text?') {
          const nonnull = new Map(lexical);
          // Only the same already-bound value narrows on the proved arm;
          // its node, name and public nullable input contract are unchanged.
          nonnull.set(other.name, {...binding, type: 'text'});
          if (condition.operator === '!==') yesScope = nonnull;
          else noScope = nonnull;
        }
      }
      const yes = expression(expr.whenTrue, yesScope, entryScope), no = expression(expr.whenFalse, noScope, entryScope);
      const joined = yes.type === no.type ? yes.type
        : [yes.type, no.type].every(isTextDomain) ? 'text?' : undefined;
      if (test.type !== 'bool' || !joined) fail('conditional requires a Boolean and two matching business value types');
      return {type: joined, node: internal('if', {cond: test.node, then: yes.node, else: no.node})};
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
  const types = Object.fromEntries(Array.from(boundRecords.values(), record => [record.name, {
    fields: Object.fromEntries(Array.from(record.fields, ([name, domain]) => [name, parameter(domain)])),
  }]));
  return projectInput(file, entry, params, returnDomain, orderedMap, returns, body, fn.getText(sf), {types});
}

function projectInput(file, entry, params, returnDomain, orderedMap, returns, body, excerpt, typed = {}) {
  const binding = 'Entry.run';
  const moduleFile = 'Module.ts';
  const javaParams = params.map(param => param.record ? 'Object' : carrier(param.domain));
  const wireType = param => param.record ? param.record.wire : valueType(param.domain);
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
    parameter_value_types: {run: Object.fromEntries(params.filter(p => p.record || p.domain === readonlyVector || p.domain === 'unknown')
        .map(p => [p.name, wireType(p)]))},
    entry_contexts: {[binding]: {
      function: file + '#' + entry, returns,
      ...(typed.externalResults && {external_results: typed.externalResults}),
      ...(typed.environment && {environment: typed.environment}),
      ...(typed.types && Object.keys(typed.types).length && {types: typed.types}),
      parameters: Object.fromEntries(params.map(param => [param.publicName, {
        position: param.position, type: param.record?.name ?? parameter(param.domain),
      }])),
    }},
    functions: {run: {
      java: binding + '(' + javaParams.join(',') + ')',
      params: params.map(param => ({name: param.name, type: wireType(param)})),
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
  const [kernel, rootArg, flag, locator = ''] = process.argv.slice(2);
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
