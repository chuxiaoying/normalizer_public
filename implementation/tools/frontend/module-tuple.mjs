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
  const functions = sf.statements.filter(ts.isFunctionDeclaration);
  const helper = functions.find((dec) => dec !== fn);
  const [bind, ret, ...other] = fn.body.statements;
  const declarations = bind && ts.isVariableStatement(bind)
    && (bind.declarationList.flags & ts.NodeFlags.Const) && bind.declarationList.declarations;
  const decl = declarations?.[0];
  const call = decl?.initializer && unwrap(decl.initializer);
  const typeName = helper?.type;
  const closedSync = (dec) => dec?.name && uid(dec.name.text) && dec.body
    && !dec.asteriskToken && !has(dec, ts.SyntaxKind.AsyncKeyword);
  if (functions.length !== 2 || !closedSync(fn) || !closedSync(helper)
      || fn.type?.kind !== ts.SyntaxKind.NumberKeyword || has(helper, ts.SyntaxKind.ExportKeyword)
      || fn.name.text === helper.name.text || other.length || declarations?.length !== 1
      || !ret || !ts.isReturnStatement(ret) || !ret.expression || !ts.isArrayBindingPattern(decl.name)
      || !call || !ts.isCallExpression(call) || call.questionDotToken
      || !ts.isIdentifier(call.expression) || call.expression.text !== helper.name.text
      || call.arguments.length !== helper.parameters.length || !typeName
      || !ts.isTypeReferenceNode(typeName) || !ts.isIdentifier(typeName.typeName)
      || !sf.statements.some((item) => ts.isTypeAliasDeclaration(item)
        && item.name.text === typeName.typeName.text && ts.isTupleTypeNode(item.type))) {
    fail('expected a closed typed tuple entry and one local tuple source helper');
  }
  const params = typedBooleanParameters(fn);
  const inputScope = new Map(params.map(name => [name, variable(name)]));
  const helperParams = typedBooleanParameters(helper);

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
    const fields = Object.create(null);
    const closedRecord = ts.isObjectLiteralExpression(expr) && expr.properties.every((prop) => {
      if (!ts.isPropertyAssignment(prop) || !ts.isIdentifier(prop.name) || !uid(prop.name.text)
          || prop.name.text === '__proto__' || Object.hasOwn(fields, prop.name.text)
          || ![ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword].includes(prop.initializer.kind)) return false;
      fields[prop.name.text] = prop.initializer.kind === ts.SyntaxKind.TrueKeyword;
      return true;
    });
    if (closedRecord) return {kind: 'record', fields};
    fail('tuple requires literal slots or distinct constant Boolean record fields');
  }
  function tupleReturn(st) {
    const expression = ts.isReturnStatement(st) && st.expression && unwrap(st.expression);
    if (expression && ts.isArrayLiteralExpression(expression)) {
      return {kind: 'literal', items: expression.elements.map(slot)};
    }
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
    return tupleReturn(head);
  }

  const positions = new Map();
  for (const [index, element] of decl.name.elements.entries()) {
    if (!ts.isIdentifier(element.name) || !uid(element.name.text) || element.initializer
        || positions.has(element.name.text) || inputScope.has(element.name.text)) fail('unsupported tuple destructuring binding');
    positions.set(element.name.text, {index, rest: Boolean(element.dotDotDotToken)});
  }
  const observed = new Map();
  let nextProbe = 0;
  function observe(kind, fields) {
    const key = JSON.stringify([kind, fields]);
    if (!observed.has(key)) {
      // Every lower index has already been allocated or reserved by a formal.
      observed.set(key, {kind, name: 'tuple_probe_' + nextProbe++, ...fields});
    }
    return variable(observed.get(key).name);
  }
  const position = expr => {
    const value = unwrap(expr);
    if (ts.isIdentifier(value)) return positions.get(value.text);
  };
  function truth(expr) {
    expr = unwrap(expr);
    const simple = lowerBool(expr, inputScope);
    if (simple) return simple;
    if (ts.isBinaryExpression(expr)) {
      if (expr.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) {
        return choose(truth(expr.left), truth(expr.right), literal('bool', false));
      }
      if ([ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(expr.operatorToken.kind)) {
        const sides = [unwrap(expr.left), unwrap(expr.right)];
        const undefinedPosition = sides.findIndex((value) => ts.isIdentifier(value) && value.text === 'undefined');
        const subject = sides[1 - undefinedPosition];
        const item = subject && position(subject);
        const tested = observe('is_undefined', {index: item.index});
        return tested;
      }
    }
    if (ts.isPropertyAccessExpression(expr) && expr.questionDotToken) {
      const item = position(expr.expression);
      const type = checker.getTypeAtLocation(expr);
      const alternatives = type.types;
      const boolean = flags => Boolean(flags & ts.TypeFlags.BooleanLike);
      if (!alternatives.some(alt => boolean(alt.flags))
          || !alternatives.every(alt => boolean(alt.flags) || alt.flags & ts.TypeFlags.Undefined)) {
        fail('optional tuple condition must be a Boolean flag');
      }
      return observe('optional_flag', {index: item.index, field: expr.name.text});
    }
    fail('unsupported tuple helper Boolean expression');
  }
  function numeric(expr) {
    expr = unwrap(expr);
    if (ts.isConditionalExpression(expr)) return choose(truth(expr.condition), numeric(expr.whenTrue), numeric(expr.whenFalse));
    if (ts.isNumericLiteral(expr) && Number.isSafeInteger(Number(expr.text))) return literal('number', Number(expr.text));
    if (ts.isPropertyAccessExpression(expr) && !expr.questionDotToken && expr.name.text === 'length') {
      const item = position(expr.expression);
      if (item?.rest) return observe('rest_length', {from: item.index});
    }
  }
  const body = numeric(ret.expression);
  return {params, excerpt: fn.getText(sf), result: n('value.observe_tuple', {
    tuple: plan([...helper.body.statements]), observations: [...observed.values()], body,
  })};
}
