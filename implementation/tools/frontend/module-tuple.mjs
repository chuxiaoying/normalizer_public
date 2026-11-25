// Shared module front end for a typed, nonescaping tuple and its observations.
// The emitted plan keeps source branches and literal slots; the kernel executes it.
import ts from 'typescript';

const n = (op, fields) => ({op, ...fields});
const variable = name => n('var', {name});
const literal = (type, value) => n('const', {type, value});
const choose = (cond, then, otherwise) => n('if', {cond, then, else: otherwise});
const has = (node, kind) => node.modifiers?.some(m => m.kind === kind);
const fail = message => { throw new Error('[NORM-TS-MODULE] ' + message); };
const unwrap = node => {
  while (ts.isParenthesizedExpression(node)) node = node.expression;
  return node;
};
const uid = x => typeof x === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(x) && x !== 'undefined';

export function compileTypedTuple(source, entry, physicalPath) {
  const sf = ts.createSourceFile(physicalPath, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  if (sf.parseDiagnostics.length) fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  if (sf.referencedFiles.length || sf.typeReferenceDirectives.length || sf.libReferenceDirectives.length
      || sf.statements.some(node => !ts.isFunctionDeclaration(node) && !ts.isTypeAliasDeclaration(node))) {
    fail('typed tuple module requires closed function and type declarations');
  }
  const funcs = new Map();
  for (const dec of sf.statements.filter(ts.isFunctionDeclaration)) {
    if (!dec.name || !uid(dec.name.text) || funcs.has(dec.name.text) || !dec.body
        || dec.asteriskToken || has(dec, ts.SyntaxKind.AsyncKeyword)) fail('expected distinct closed synchronous functions');
    funcs.set(dec.name.text, dec);
  }
  const fn = funcs.get(entry);
  if (!fn || !has(fn, ts.SyntaxKind.ExportKeyword) || fn.type?.kind !== ts.SyntaxKind.NumberKeyword) {
    fail('typed tuple module requires an exported number function');
  }
  function paramsOf(declaration) {
    const bound = new Set();
    return declaration.parameters.map(param => {
      if (!ts.isIdentifier(param.name) || !uid(param.name.text) || bound.has(param.name.text)
          || param.initializer || param.dotDotDotToken || param.questionToken
          || param.type?.kind !== ts.SyntaxKind.BooleanKeyword) fail('tuple functions require distinct Boolean parameters');
      bound.add(param.name.text);
      return param.name.text;
    });
  }
  const params = paramsOf(fn);
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
  const helperParams = paramsOf(helper);
  if (call.arguments.length !== helperParams.length) fail('tuple helper arity mismatch');

  // Reject imports and reference directives before asking the checker for the
  // typed program. No transpilation or evaluation of the user module occurs.
  let hasImport = false;
  const inspect = node => {
    if (ts.isImportTypeNode(node) || node.kind === ts.SyntaxKind.ImportKeyword) hasImport = true;
    ts.forEachChild(node, inspect);
  };
  inspect(sf);
  if (hasImport) fail('typed tuple module cannot import outside declarations');
  const options = {strict: true, noEmit: true, noResolve: true, types: [], target: ts.ScriptTarget.ES2022};
  const host = ts.createCompilerHost(options);
  const readSource = host.getSourceFile.bind(host);
  host.getSourceFile = (name, ...args) => name === physicalPath ? sf : readSource(name, ...args);
  const checked = ts.createProgram([physicalPath], options, host);
  const checker = checked.getTypeChecker();
  const errors = [...checked.getSyntacticDiagnostics(), ...checked.getSemanticDiagnostics()];
  if (errors.length) fail(ts.flattenDiagnosticMessageText(errors[0].messageText, '\n'));
  if (!helper.type || !ts.isTypeReferenceNode(helper.type) || !ts.isIdentifier(helper.type.typeName)
      || !sf.statements.some(item => ts.isTypeAliasDeclaration(item)
        && item.name.text === helper.type.typeName.text && ts.isTupleTypeNode(item.type))) {
    fail('tuple helper needs a declared tuple type');
  }

  function simpleBool(expr, scope) {
    expr = unwrap(expr);
    if (ts.isIdentifier(expr) && scope.has(expr.text)) return scope.get(expr.text);
    if (expr.kind === ts.SyntaxKind.TrueKeyword || expr.kind === ts.SyntaxKind.FalseKeyword) {
      return literal('bool', expr.kind === ts.SyntaxKind.TrueKeyword);
    }
    if (ts.isPrefixUnaryExpression(expr) && expr.operator === ts.SyntaxKind.ExclamationToken) {
      return n('not', {value: simpleBool(expr.operand, scope)});
    }
    fail('unsupported tuple helper Boolean expression');
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
  function plan(statements) {
    const [head, ...tail] = statements;
    if (!head) fail('tuple helper has no return on a path');
    if (ts.isIfStatement(head) && !head.elseStatement) {
      return {kind: 'branch', cond: simpleBool(head.expression, scope), yes: tupleReturn(head.thenStatement), no: plan(tail)};
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
    return ts.isIdentifier(value) ? positions.get(value.text) : null;
  };
  function truth(expr) {
    expr = unwrap(expr);
    if (ts.isBinaryExpression(expr)) {
      if (expr.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) {
        return choose(truth(expr.left), truth(expr.right), literal('bool', false));
      }
      if ([ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(expr.operatorToken.kind)) {
        const isUndefined = value => ts.isIdentifier(unwrap(value)) && unwrap(value).text === 'undefined';
        const item = position(isUndefined(expr.left) ? expr.right : isUndefined(expr.right) ? expr.left : expr);
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
