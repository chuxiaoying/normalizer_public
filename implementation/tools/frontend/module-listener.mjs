// Closed lowering for a local listener slot and typed synchronous calls.
// A registration writes the lexical callback; dispatch evaluates its body.
import ts from 'typescript';
import { unwrapParens } from './ir.mjs';
import { checkedTypeScript, failTypedModule, typedParameterNames } from './typed-module.mjs';

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
  const checker = checkedTypeScript(physicalPath, sf);
  const params = typedParameterNames(fn);
  const contracts = new Map(), ports = {}, externalResults = {};
  for (const host of sf.statements) {
    if (host === fn) continue;
    const [root] = host.declarationList.declarations;
    const markers = ts.getJSDocTags(host);
    if (markers.some(tag => tag.tagName.text !== 'external')) fail('unsupported listener host contract tag');
    for (const signature of root.type.members) {
      const target = root.name.text + '.' + signature.name.text;
      const port = markers.length ? 'sync_' + contracts.size : null;
      contracts.set(target, { declaration: root, port });
      if (port) {
        ports[port] = {origin: {path: 'Module.ts', excerpt: host.getText(sf)}, endpoint: target,
          params: [], returns: 'unit', throws: [], heap_effect: 'none', argument_observation: 'immutable_value', effect: 'sync'};
        externalResults[port] = 'void';
      }
    }
  }

  const [slot, register, call, ...dispatches] = fn.body.statements;
  const declarations = slot && ts.isVariableStatement(slot) && slot.declarationList.flags & ts.NodeFlags.Let
    && slot.declarationList.declarations;
  const pending = declarations?.[0], slotType = pending?.type;
  const registrar = register && ts.isFunctionDeclaration(register) && register;
  const parameter = registrar?.parameters?.[0];
  const registration = call && statementCall(call);
  const arrow = registration?.arguments[0];
  const body = arrow && ts.isArrowFunction(arrow) && ts.isBlock(arrow.body) && arrow.body.statements;
  const returns = body?.at?.(-1);
  if (fn.asteriskToken || fn.modifiers?.some((m) => is(m, ts.SyntaxKind.AsyncKeyword))
      || declarations?.length !== 1 || pending.initializer || !ts.isIdentifier(pending.name)
      || !slotType || !ts.isUnionTypeNode(slotType) || slotType.types.length !== 2
      || !slotType.types.some(callbackType) || !slotType.types.some((item) => is(item, ts.SyntaxKind.UndefinedKeyword))
      || !registrar?.name || !registrar.body || registrar.asteriskToken
      || registrar.modifiers || registrar.parameters.length !== 1
      || !ts.isIdentifier(parameter.name) || !callbackType(parameter.type) || parameter.initializer
      || parameter.dotDotDotToken || parameter.questionToken
      || !registration || registration.questionDotToken || registration.arguments.length !== 1
      || !named(registration.expression, registrar.name.text) || !body || arrow.modifiers
      || arrow.parameters.length || !returns || !ts.isReturnStatement(returns)
      || !returns.expression || !ts.isStringLiteral(returns.expression)) {
    fail('listener needs a closed synchronous entry, callback slot and registration');
  }
  const callback = body.slice(0, -1);

  const declarationOf = node => checker.getSymbolAtLocation(node)?.valueDeclaration;
  let serial = 0;
  const steps = [];
  function statementExpression(st) {
    if (ts.isExpressionStatement(st)) return st.expression;
  }
  function statementCall(st) {
    const expr = statementExpression(st);
    if (expr && ts.isCallExpression(expr)) return expr;
  }
  function callExpression(st) {
    const expr = statementCall(st);
    if (expr && !expr.questionDotToken && !expr.arguments.length) return expr.expression;
  }
  function lowerExternal(st, condition) {
    const callee = callExpression(st);
    const target = callee.expression.text + '.' + callee.name.text;
    const contract = contracts.get(target);
    // A callback runs at the dispatch site, but its names bind at definition.
    const declaration = declarationOf(callee.expression);
    if (declaration && contract && declaration !== contract.declaration) {
      fail('local name shadows declared listener host');
    }
    steps.push({op: 'call_sync', port: contract.port, target, site: 'sync_call_' + serial++, condition});
  }
  const executeCallback = condition => { for (const st of callback) lowerExternal(st, condition); };
  let saved = false;
  for (const st of register.body.statements) {
    const expr = statementExpression(st);
    if (expr && ts.isBinaryExpression(expr) && is(expr.operatorToken, ts.SyntaxKind.EqualsToken)
        && named(expr.left, pending.name.text) && named(expr.right, parameter.name.text)) {
      if (declarationOf(expr.left) !== pending || declarationOf(expr.right) !== parameter) {
        fail('listener registration must write the entry callback slot from its callback parameter');
      }
      saved = true;
    } else if (named(callExpression(st), parameter.name.text)) {
      executeCallback(null);
    } else {
      lowerExternal(st, null);
    }
  }
  if (!saved) fail('listener callback is not registered in the local slot');
  function numericTruth(node) {
    node = unwrapParens(node);
    if (ts.isPrefixUnaryExpression(node)
        && (node.operator === ts.SyntaxKind.PlusToken || node.operator === ts.SyntaxKind.MinusToken)) {
      return numericTruth(node.operand); // the sign does not change whether the exact value is zero
    }
    if (!ts.isNumericLiteral(node)) fail('listener dispatch condition must be a closed Boolean');
    const spelling = node.getText().replaceAll('_', '');
    if (/^0[xob]/i.test(spelling)) return /[1-9a-f]/i.test(spelling.slice(2));
    // No Number conversion: exponent digits do not make a zero significand nonzero.
    return /[1-9]/.test(spelling.split(/[eE]/, 1)[0]);
  }
  function gate(node) {
    node = unwrapParens(node);
    if (ts.isIdentifier(node) && params.includes(node.text)) return {op: 'var', name: node.text};
    if (is(node, ts.SyntaxKind.TrueKeyword) || is(node, ts.SyntaxKind.FalseKeyword)) return value('bool', is(node, ts.SyntaxKind.TrueKeyword));
    if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.ExclamationToken) {
      const operand = gate(node.operand);
      return operand.op === 'const' ? value('bool', !operand.value) : {op: 'not', value: operand};
    }
    return value('bool', numericTruth(node));
  }
  for (const st of dispatches) {
    if (!ts.isIfStatement(st) || st.elseStatement)
      fail('listener dispatch requires a single guarded branch');
    const condition = gate(st.expression);
    for (const _ of blockStatements(st.thenStatement)) executeCallback(condition);
  }
  steps.push({op: 'return', value: value('unit', null)});
  return {params, excerpt: fn.getText(sf), ports, externalResults,
    result: {op: 'settled', returns: 'unit', steps}};
}
