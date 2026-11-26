// Closed lowering for a local listener slot and typed synchronous calls.
// A registration writes the lexical callback; dispatch evaluates its body.
import ts from 'typescript';
import { unwrapParens } from './ir.mjs';
import { checkedTypeScript, failTypedModule, typedParameterNames } from './typed-module.mjs';
import { normalizerMarkers, singleAnnotatedBinding } from './module-projection.mjs';

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

export function compileTypedListener(physicalPath, sf, fn, modulePath) {
  const checker = checkedTypeScript(physicalPath, sf);
  const params = typedParameterNames(fn);
  const contracts = new Map(), ports = {}, externalResults = {};
  for (const host of sf.statements) {
    if (host === fn) continue;
    if (!ts.isVariableStatement(host) || !host.modifiers?.some(m => is(m, ts.SyntaxKind.DeclareKeyword))) {
      fail('listener requires a closed top-level host declaration');
    }
    const root = singleAnnotatedBinding(host, 'listener host must mark one named declaration', fail);
    const markers = normalizerMarkers(host, fail);
    if (ts.getJSDocTags(host).some(tag => ['throws', 'exception'].includes(tag.tagName.text))) {
      fail('unsupported listener host contract tag');
    }
    if (markers.size && !markers.has('external') || root.initializer || !root.type || !ts.isTypeLiteralNode(root.type)) {
      fail('listener host requires a closed declared receiver interface or source shape');
    }
    for (const signature of root.type.members) {
      if (!ts.isMethodSignature(signature) || !ts.isIdentifier(signature.name) ||
          signature.questionToken || signature.typeParameters?.length ||
          signature.parameters.length || !is(signature.type, ts.SyntaxKind.VoidKeyword)) {
        fail('listener requires a named required zero-argument void member');
      }
      const target = root.name.text + '.' + signature.name.text;
      const port = markers.has('external') ? 'sync_' + contracts.size : null;
      const endpoint = modulePath + '#' + target;
      if (contracts.has(target)) fail('duplicate listener host member ' + target);
      contracts.set(target, { declaration: root, signature, port, endpoint });
      if (port) {
        ports[port] = { origin: {path: 'Module.ts', excerpt: host.getText(sf)}, endpoint,
          params: [], returns: 'unit', throws: [], heap_effect: 'none', argument_observation: 'immutable_value', effect: 'sync' };
        externalResults[port] = 'void';
      }
    }
  }

  const statements = [...fn.body.statements];
  const registrarIndex = statements.findIndex(ts.isFunctionDeclaration);
  const slots = statements.slice(0, registrarIndex).flatMap(st => ts.isVariableStatement(st)
    && st.declarationList.flags & ts.NodeFlags.Let ? [...st.declarationList.declarations] : []);
  const [register, call, ...dispatches] = registrarIndex < 0 ? [] : statements.slice(registrarIndex);
  const declarationOf = node => checker.getSymbolAtLocation(node)?.valueDeclaration;
  const observedSlots = new Set();
  for (const st of dispatches) {
    const scan = node => {
      if (ts.isCallExpression(node)) {
        let target = unwrapParens(node.expression);
        if (ts.isNonNullExpression(target)) target = unwrapParens(target.expression);
        if (ts.isIdentifier(target)) {
          const dec = declarationOf(target);
          if (slots.includes(dec)) observedSlots.add(dec);
        }
      }
      ts.forEachChild(node, scan);
    };
    scan(st);
  }
  const pending = [...observedSlots][0];
  const registrar = register && ts.isFunctionDeclaration(register) && register;
  const parameter = registrar?.parameters?.[0];
  const registration = call && statementCall(call);
  const arrow = registration?.arguments[0];
  const body = arrow && ts.isArrowFunction(arrow) && ts.isBlock(arrow.body) && arrow.body.statements;
  const returns = body?.at?.(-1);
  if (fn.asteriskToken || fn.modifiers?.some((m) => is(m, ts.SyntaxKind.AsyncKeyword))
      || registrarIndex < 1 || observedSlots.size > 1
      || !statements.slice(0, registrarIndex).every(st => ts.isVariableStatement(st)
          && st.declarationList.flags & ts.NodeFlags.Let
          && st.declarationList.declarations.every(dec => !dec.initializer && ts.isIdentifier(dec.name)
            && dec.type && ts.isUnionTypeNode(dec.type) && dec.type.types.length === 2
            && dec.type.types.some(callbackType) && dec.type.types.some(t => is(t, ts.SyntaxKind.UndefinedKeyword))))
      || !registrar?.name || !registrar.body || registrar.asteriskToken
      || registrar.modifiers || registrar.parameters.length !== 1
      || !ts.isIdentifier(parameter.name) || !callbackType(parameter.type) || parameter.initializer
      || parameter.dotDotDotToken || parameter.questionToken
      || !registration || registration.questionDotToken || registration.arguments.length !== 1
      || !named(registration.expression, registrar.name.text) || declarationOf(registration.expression) !== registrar || !body || arrow.modifiers
      || arrow.parameters.some(p => !p.questionToken || p.initializer || p.dotDotDotToken || !ts.isIdentifier(p.name))
      || !returns || !ts.isReturnStatement(returns) || !returns.expression
      || !(checker.getTypeAtLocation(returns.expression).flags & ts.TypeFlags.StringLike
          || checker.getTypeAtLocation(returns.expression).types?.every(alt => alt.flags & ts.TypeFlags.StringLike))) {
    fail('listener needs a closed synchronous entry, callback slot and registration');
  }
  const callback = body.slice(0, -1);
  function pureReturn(node) {
    node = unwrapParens(node);
    return ts.isStringLiteral(node) || ts.isConditionalExpression(node)
      && ts.isIdentifier(unwrapParens(node.condition))
      && fn.parameters.includes(declarationOf(unwrapParens(node.condition)))
      && pureReturn(node.whenTrue) && pureReturn(node.whenFalse);
  }
  if (!pureReturn(returns.expression)) fail('listener callback return must be a closed pure value');

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
    const raw = callExpression(st);
    const callee = raw && unwrapParens(raw);
    if (!callee || !ts.isPropertyAccessExpression(callee) || !ts.isIdentifier(callee.expression) || callee.questionDotToken) {
      fail('listener body requires a directly observed call');
    }
    const target = callee.expression.text + '.' + callee.name.text;
    const contract = contracts.get(target);
    // A callback runs at the dispatch site, but its names bind at definition.
    const declaration = declarationOf(callee.expression);
    if (contract && (declaration !== contract.declaration ||
        !checker.getSymbolAtLocation(callee.name)?.declarations?.includes(contract.signature))) {
      fail('local name shadows declared listener host');
    }
    steps.push({op: 'call_sync', port: contract?.port ?? null, target: contract?.port ? contract.endpoint : target,
      site: 'sync_call_' + serial++, condition});
  }
  const executeCallback = condition => { for (const st of callback) lowerExternal(st, condition); };
  let saved = false;
  for (const st of register.body.statements) {
    const expr = statementExpression(st);
    if (expr && ts.isBinaryExpression(expr) && is(expr.operatorToken, ts.SyntaxKind.EqualsToken)
        && ts.isIdentifier(expr.left) && ts.isIdentifier(expr.right)
        && declarationOf(expr.right) === parameter) {
      const left = declarationOf(expr.left);
      if (pending && left === pending) saved = true;
      else if (left !== parameter && !slots.includes(left)) fail('listener registration must write a known local slot');
    } else if (callExpression(st) && declarationOf(callExpression(st)) === parameter) {
      executeCallback(null);
    } else if (ts.isIfStatement(st) && !st.elseStatement && ts.isBinaryExpression(st.expression)
        && is(st.expression.operatorToken, ts.SyntaxKind.EqualsEqualsEqualsToken)
        && ts.isIdentifier(st.expression.left) && ts.isIdentifier(st.expression.right)) {
      const a = declarationOf(st.expression.left), b = declarationOf(st.expression.right);
      if (!saved || !((a === parameter && b === pending) || (b === parameter && a === pending))) {
        fail('listener registration requires a proven local callback equality');
      }
      for (const branch of blockStatements(st.thenStatement)) lowerExternal(branch, null);
    } else {
      lowerExternal(st, null);
    }
  }
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
    if (ts.isIdentifier(node) && fn.parameters.includes(declarationOf(node))
        && checker.getTypeAtLocation(node).flags & ts.TypeFlags.BooleanLike) return {op: 'var', name: node.text};
    if (ts.isBinaryExpression(node)
        && [ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(node.operatorToken.kind)) {
      const left = unwrapParens(node.left), right = unwrapParens(node.right);
      const sel = ts.isConditionalExpression(left) && ts.isStringLiteral(right) ? left
        : ts.isConditionalExpression(right) && ts.isStringLiteral(left) ? right : null;
      if (sel && ts.isStringLiteral(sel.whenTrue) && ts.isStringLiteral(sel.whenFalse)) {
        const compare = sel === left ? right.text : left.text;
        const neq = node.operatorToken.kind === ts.SyntaxKind.ExclamationEqualsEqualsToken;
        const yes = (sel.whenTrue.text === compare) !== neq;
        const no = (sel.whenFalse.text === compare) !== neq;
        const guard = gate(sel.condition);
        if (yes === no) return value('bool', yes);
        return yes ? guard : invert(guard);
      }
    }
    if (is(node, ts.SyntaxKind.TrueKeyword) || is(node, ts.SyntaxKind.FalseKeyword)) return value('bool', is(node, ts.SyntaxKind.TrueKeyword));
    if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.ExclamationToken) {
      const operand = gate(node.operand);
      return operand.op === 'const' ? value('bool', !operand.value) : {op: 'not', value: operand};
    }
    return value('bool', numericTruth(node));
  }
  function dispatch(st, condition) {
    const raw = callExpression(st);
    let expr = raw && unwrapParens(raw);
    if (expr && ts.isNonNullExpression(expr)) expr = unwrapParens(expr.expression);
    if (pending && named(expr, pending.name.text) && declarationOf(expr) === pending) {
      if (saved) executeCallback(condition);
      else steps.push({op: 'throw_uninitialized_callback', condition});
    } else {
      lowerExternal(st, condition);
    }
  }
  const invert = cond => cond.op === 'const' ? value('bool', !cond.value) : {op: 'not', value: cond};
  for (const st of dispatches) {
    if (!ts.isIfStatement(st)) fail('listener dispatch requires an observed guarded branch');
    const condition = gate(st.expression);
    for (const branch of blockStatements(st.thenStatement)) dispatch(branch, condition);
    if (st.elseStatement) {
      for (const branch of blockStatements(st.elseStatement)) dispatch(branch, invert(condition));
    }
  }
  steps.push({op: 'return', value: value('unit', null)});
  return {params, excerpt: fn.getText(sf), ports, externalResults,
    exceptions: steps.some(step => step.op === 'throw_uninitialized_callback') ? {TypeError: 'unit'} : {},
    result: {op: 'settled', returns: 'unit', steps}};
}
