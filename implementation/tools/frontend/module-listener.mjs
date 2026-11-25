// Closed lowering for a local listener slot and typed synchronous calls.
// A registration writes the lexical callback; dispatch evaluates its body.
import ts from 'typescript';
import { checkedTypeScript, failTypedModule, typedBooleanParameters } from './typed-module.mjs';

const fail = failTypedModule;
const is = (node, kind) => node?.kind === kind;
const named = (node, text) => !!node && ts.isIdentifier(node) && node.text === text;
const value = (type, x) => ({op: 'const', type, value: x});
const nominal = sig => ts.isFunctionTypeNode(sig) && sig.parameters.length === 0
  && is(sig.type, ts.SyntaxKind.StringKeyword);
function callbackType(type) {
  return type && (ts.isParenthesizedTypeNode(type) ? callbackType(type.type) : nominal(type));
}
function blockStatements(node) {
  return ts.isBlock(node) ? [...node.statements] : [node];
}

export function compileTypedListener(physicalPath, sf, fn) {
  checkedTypeScript(physicalPath, sf, node => node === fn || ts.isVariableStatement(node));
  if (!fn.body || fn.asteriskToken || fn.modifiers?.some(m => is(m, ts.SyntaxKind.AsyncKeyword))) {
    fail('listener entry requires a synchronous source definition');
  }
  const params = typedBooleanParameters(fn);
  const contracts = new Map(), ports = {}, externalResults = {};
  for (const host of sf.statements) {
    if (host === fn) continue;
    const [root, ...extra] = host.declarationList.declarations;
    if (!root || extra.length || root.initializer || !ts.isIdentifier(root.name)
        || !(host.declarationList.flags & ts.NodeFlags.Const)
        || !host.modifiers?.some(m => is(m, ts.SyntaxKind.DeclareKeyword))
        || !root.type || !ts.isTypeLiteralNode(root.type)) fail('listener host needs a closed typed declaration');
    const markers = ts.getJSDocTags(host);
    if (markers.some(tag => tag.tagName.text !== 'external')) fail('unsupported listener host contract tag');
    if (markers.length > 1 || markers.some(tag => tag.comment)) fail('invalid listener external marker');
    for (const signature of root.type.members) {
      if (!ts.isMethodSignature(signature) || !ts.isIdentifier(signature.name) || signature.questionToken
          || signature.parameters.length || signature.typeParameters?.length || !is(signature.type, ts.SyntaxKind.VoidKeyword)) {
        fail('listener external must declare a zero-argument void method');
      }
      const target = root.name.text + '.' + signature.name.text;
      if (contracts.has(target)) fail('duplicate listener target');
      const port = markers.length ? 'sync_' + contracts.size : null;
      contracts.set(target, port);
      if (port) {
        ports[port] = {origin: {path: 'Module.ts', excerpt: host.getText(sf)}, endpoint: target,
          params: [], returns: 'unit', throws: [], heap_effect: 'none', argument_observation: 'immutable_value', effect: 'sync'};
        externalResults[port] = 'void';
      }
    }
  }

  const [slot, register, call, ...dispatches] = fn.body.statements;
  const pending = slot && ts.isVariableStatement(slot) && slot.declarationList.flags & ts.NodeFlags.Let
    && slot.declarationList.declarations.length === 1 ? slot.declarationList.declarations[0] : null;
  if (!pending || pending.initializer || !ts.isIdentifier(pending.name) || !pending.type
      || !ts.isUnionTypeNode(pending.type) || pending.type.types.length !== 2
      || !pending.type.types.some(callbackType) || !pending.type.types.some(t => is(t, ts.SyntaxKind.UndefinedKeyword))) {
    fail('listener needs an uninitialized typed local callback slot');
  }
  if (!register || !ts.isFunctionDeclaration(register) || !register.name || !register.body
      || register.asteriskToken || register.modifiers?.length || register.parameters.length !== 1) {
    fail('listener needs a closed local registration function');
  }
  const [parameter] = register.parameters;
  if (!ts.isIdentifier(parameter.name) || !callbackType(parameter.type) || parameter.initializer
      || parameter.dotDotDotToken || parameter.questionToken) fail('listener registration needs an exact callback parameter');

  const registration = call && ts.isExpressionStatement(call) && ts.isCallExpression(call.expression) ? call.expression : null;
  const arrow = registration?.arguments[0];
  if (!registration || registration.questionDotToken || registration.arguments.length !== 1
      || !named(registration.expression, register.name.text) || !arrow || !ts.isArrowFunction(arrow)
      || arrow.modifiers?.length || arrow.parameters.length || !ts.isBlock(arrow.body)) {
    fail('listener needs a nonescaping source callback at registration');
  }
  const callback = [...arrow.body.statements];
  const returns = callback.pop();
  if (!returns || !ts.isReturnStatement(returns) || !returns.expression || !ts.isStringLiteral(returns.expression)) {
    fail('listener callback needs a closed local string return');
  }

  let serial = 0;
  const steps = [];
  function callExpression(st) {
    return ts.isExpressionStatement(st) && ts.isCallExpression(st.expression)
      && !st.expression.questionDotToken && !st.expression.arguments.length ? st.expression.expression : null;
  }
  function lowerExternal(st, condition) {
    const callee = callExpression(st);
    if (!callee || !ts.isPropertyAccessExpression(callee) || callee.questionDotToken
        || !ts.isIdentifier(callee.expression)) fail('listener effect must be a direct closed host call');
    const root = callee.expression.text;
    if ([...params, pending.name.text, register.name.text, parameter.name.text].includes(root)) {
      fail('local name shadows declared listener host');
    }
    const target = root + '.' + callee.name.text;
    if (!contracts.has(target)) fail('listener effect target has no closed source signature');
    steps.push({op: 'call_sync', port: contracts.get(target), target, site: 'sync_call_' + serial++, condition});
  }
  const executeCallback = condition => { for (const st of callback) lowerExternal(st, condition); };
  let saved = false;
  for (const st of register.body.statements) {
    const expr = ts.isExpressionStatement(st) ? st.expression : null;
    if (expr && ts.isBinaryExpression(expr) && is(expr.operatorToken, ts.SyntaxKind.EqualsToken)
        && named(expr.left, pending.name.text) && named(expr.right, parameter.name.text)) {
      if (saved) fail('listener source slot must be assigned only once');
      saved = true;
    } else if (named(callExpression(st), parameter.name.text)) {
      executeCallback(null);
    } else {
      lowerExternal(st, null);
    }
  }
  if (!saved) fail('listener callback is not registered in the local slot');
  function gate(node) {
    if (ts.isParenthesizedExpression(node)) return gate(node.expression);
    if (ts.isIdentifier(node) && params.includes(node.text)) return {op: 'var', name: node.text};
    if (is(node, ts.SyntaxKind.TrueKeyword) || is(node, ts.SyntaxKind.FalseKeyword)) return value('bool', is(node, ts.SyntaxKind.TrueKeyword));
    if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.ExclamationToken) return {op: 'not', value: gate(node.operand)};
    fail('listener dispatch condition must be a closed Boolean');
  }
  function dispatch(st, condition) {
    const callee = callExpression(st);
    if (!callee || !ts.isNonNullExpression(callee) || !named(callee.expression, pending.name.text)) {
      fail('dispatch must call the registered local callback');
    }
    executeCallback(condition);
  }
  for (const st of dispatches) {
    if (ts.isIfStatement(st) && !st.elseStatement) {
      const condition = gate(st.expression);
      for (const body of blockStatements(st.thenStatement)) dispatch(body, condition);
    } else {
      dispatch(st, null);
    }
  }
  steps.push({op: 'return', value: value('unit', null)});
  return {params, excerpt: fn.getText(sf), ports, externalResults,
    result: {op: 'settled', returns: 'unit', steps}};
}
