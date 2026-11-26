// Shared module front end for a typed, nonescaping tuple and its observations.
// The emitted plan keeps source branches and literal slots; the kernel executes it.
import ts from 'typescript';
import { unwrapParens as unwrap } from './ir.mjs';
import {checkedTypeScript, failTypedModule, typedBooleanParameters, typedIdentifier} from './typed-module.mjs';

const n = (op, fields) => ({op, ...fields});
const variable = name => n('var', {name});
const literal = (type, value) => n('const', {type, value});
const choose = (cond, then, otherwise) => n('if', {cond, then, else: otherwise});
const has = (node, kind) => node.modifiers?.some(m => m.kind === kind);
const fail = failTypedModule;
const uid = typedIdentifier;

export function compileTypedTuple(physicalPath, sf, fn) {
  const checker = checkedTypeScript(physicalPath, sf,
    node => ts.isFunctionDeclaration(node) || ts.isTypeAliasDeclaration(node));
  const funcs = new Map();
  for (const dec of sf.statements.filter(ts.isFunctionDeclaration)) {
    if (!dec.name || !uid(dec.name.text) || funcs.has(dec.name.text) || !dec.body
        || dec.asteriskToken || has(dec, ts.SyntaxKind.AsyncKeyword)) fail('expected distinct closed synchronous functions');
    funcs.set(dec.name.text, dec);
  }
  if (fn.type?.kind !== ts.SyntaxKind.NumberKeyword) fail('typed tuple module requires an exported number function');
  const params = typedBooleanParameters(fn);
  const [bind, ret, ...other] = fn.body.statements;
  if (other.length || !bind || !ret || !ts.isVariableStatement(bind) || !(bind.declarationList.flags & ts.NodeFlags.Const)
      || bind.declarationList.declarations.length !== 1 || !ts.isReturnStatement(ret) || !ret.expression) {
    fail('tuple entry needs one destructuring declaration and a return');
  }
  const decl = bind.declarationList.declarations[0];
  const call = decl.initializer && unwrap(decl.initializer);
  if (!ts.isArrayBindingPattern(decl.name) || !call || !ts.isCallExpression(call)
      || call.questionDotToken || !ts.isIdentifier(call.expression)) fail('expected a closed direct tuple helper call');
  const helper = funcs.get(call.expression.text);
  if (!helper || helper === fn || funcs.size !== 2 || has(helper, ts.SyntaxKind.ExportKeyword)) fail('tuple plan requires one local source helper');
  const inputScope = new Map(params.map(name => [name, variable(name)]));
  const helperParams = typedBooleanParameters(helper);
  if (call.arguments.length !== helperParams.length) fail('tuple helper arity mismatch');

  if (!helper.type || !ts.isTypeReferenceNode(helper.type) || !ts.isIdentifier(helper.type.typeName)
      || !sf.statements.some(item => ts.isTypeAliasDeclaration(item)
        && item.name.text === helper.type.typeName.text && ts.isTupleTypeNode(item.type))) {
    fail('tuple helper needs a declared tuple type');
  }

  function lowerBool(expr, scope) {
    expr = unwrap(expr);
    if (ts.isIdentifier(expr) && scope.has(expr.text)) return scope.get(expr.text);
    if (expr.kind === ts.SyntaxKind.TrueKeyword || expr.kind === ts.SyntaxKind.FalseKeyword) {
      return literal('bool', expr.kind === ts.SyntaxKind.TrueKeyword);
    }
    if (ts.isPrefixUnaryExpression(expr) && expr.operator === ts.SyntaxKind.ExclamationToken) {
      const operand = lowerBool(expr.operand, scope);
      if (operand) return n('not', {value: operand});
    }
  }
  function simpleBool(expr, scope) {
    return lowerBool(expr, scope) ?? fail('unsupported tuple helper Boolean expression');
  }
  function exactBool(expr) {
    if (expr?.op === 'const' && expr.type === 'bool') return expr.value;
    if (expr?.op === 'not') {
      const operand = exactBool(expr.value);
      if (operand !== undefined) return !operand;
    }
  }
  const scope = new Map(helperParams.map((name, i) => [name, simpleBool(call.arguments[i], inputScope)]));
  function slot(expr) {
    expr = unwrap(expr);
    if (ts.isIdentifier(expr) && expr.text === 'undefined') return {kind: 'undefined'};
    if (ts.isStringLiteral(expr)) return {kind: 'text', value: expr.text};
    if (!ts.isObjectLiteralExpression(expr)) fail('tuple literal requires text, undefined, or a closed Boolean record');
    const fields = Object.create(null);
    for (const prop of expr.properties) {
      if (!ts.isPropertyAssignment(prop) || !ts.isIdentifier(prop.name) || !uid(prop.name.text)
          || prop.name.text === '__proto__' || Object.hasOwn(fields, prop.name.text)
          || ![ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword].includes(prop.initializer.kind)) {
        fail('tuple record requires distinct constant Boolean fields');
      }
      fields[prop.name.text] = prop.initializer.kind === ts.SyntaxKind.TrueKeyword;
    }
    return {kind: 'record', fields};
  }
  function tupleReturn(st) {
    if (ts.isBlock(st) && st.statements.length === 1) return tupleReturn(st.statements[0]);
    if (!ts.isReturnStatement(st) || !st.expression || !ts.isArrayLiteralExpression(unwrap(st.expression))) {
      fail('tuple helper path must return a literal tuple');
    }
    return {kind: 'literal', items: unwrap(st.expression).elements.map(slot)};
  }
  function plan(statements, fallthrough = () => fail('tuple helper has no return on a path')) {
    const [head, ...tail] = statements;
    if (!head) return fallthrough();
    if (ts.isIfStatement(head)) {
      const cond = head.elseStatement ? lowerBool(head.expression, scope) : simpleBool(head.expression, scope);
      const known = exactBool(cond);
      const next = () => plan(tail, fallthrough);
      if (known !== undefined) {
        // The closed TypeScript source was checked before picking a reachable arm.
        const picked = known ? head.thenStatement : head.elseStatement;
        if (!picked) return next();
        return plan(ts.isBlock(picked) ? [...picked.statements] : [picked], next);
      }
      if (!head.elseStatement) {
        return {kind: 'branch', cond, yes: tupleReturn(head.thenStatement), no: next()};
      }
    }
    if (tail.length) fail('unsupported statements after tuple return');
    return tupleReturn(head);
  }

  const positions = new Map();
  for (const [index, element] of decl.name.elements.entries()) {
    if (ts.isOmittedExpression(element)) continue;
    if (!ts.isIdentifier(element.name) || !uid(element.name.text) || element.initializer
        || positions.has(element.name.text) || inputScope.has(element.name.text)) fail('unsupported tuple destructuring binding');
    positions.set(element.name.text, {index, rest: Boolean(element.dotDotDotToken)});
  }
  const observed = new Map();
  function observe(kind, fields) {
    const key = JSON.stringify([kind, fields]);
    if (!observed.has(key)) {
      let i = observed.size;
      while (inputScope.has('tuple_probe_' + i)
          || [...observed.values()].some(value => value.name === 'tuple_probe_' + i)) i++;
      observed.set(key, {kind, name: 'tuple_probe_' + i, ...fields});
    }
    return variable(observed.get(key).name);
  }
  const position = expr => {
    const value = unwrap(expr);
    if (ts.isIdentifier(value)) return positions.get(value.text);
  };
  function truth(expr) {
    expr = unwrap(expr);
    if (ts.isBinaryExpression(expr)) {
      if (expr.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) {
        return choose(truth(expr.left), truth(expr.right), literal('bool', false));
      }
      if ([ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(expr.operatorToken.kind)) {
        const isUndefined = value => ts.isIdentifier(unwrap(value)) && unwrap(value).text === 'undefined';
        const subject = isUndefined(expr.left) ? expr.right : isUndefined(expr.right) ? expr.left : undefined;
        const item = subject && position(subject);
        if (!item || item.rest) fail('undefined test needs one tuple slot');
        const tested = observe('is_undefined', {index: item.index});
        return expr.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken ? tested : n('not', {value: tested});
      }
    }
    if (ts.isPropertyAccessExpression(expr) && expr.questionDotToken) {
      const item = position(expr.expression);
      if (!item || item.rest || !uid(expr.name.text)) fail('optional flag needs one tuple slot');
      const type = checker.getTypeAtLocation(expr);
      const alternatives = type.isUnion() ? type.types : [type];
      const boolean = flags => Boolean(flags & ts.TypeFlags.BooleanLike);
      if (!alternatives.some(alt => boolean(alt.flags))
          || !alternatives.every(alt => boolean(alt.flags) || alt.flags & ts.TypeFlags.Undefined)) {
        fail('optional tuple condition must be a Boolean flag');
      }
      return observe('optional_flag', {index: item.index, field: expr.name.text});
    }
    return simpleBool(expr, inputScope);
  }
  function numeric(expr) {
    expr = unwrap(expr);
    if (ts.isConditionalExpression(expr)) return choose(truth(expr.condition), numeric(expr.whenTrue), numeric(expr.whenFalse));
    if (ts.isNumericLiteral(expr) && Number.isSafeInteger(Number(expr.text))) return literal('number', Number(expr.text));
    if (ts.isPropertyAccessExpression(expr) && !expr.questionDotToken && expr.name.text === 'length') {
      const item = position(expr.expression);
      if (item?.rest) return observe('rest_length', {from: item.index});
    }
    fail('unsupported tuple number expression');
  }
  const body = numeric(ret.expression);
  return {params, excerpt: fn.getText(sf), result: n('value.observe_tuple', {
    tuple: plan([...helper.body.statements]), observations: [...observed.values()], body,
  })};
}
