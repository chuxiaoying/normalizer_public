// A checked native TypeScript type-test boundary. A conversion is transparent
// only when the branch already proves the same acquired native value.
import ts from 'typescript';
import {isLocalExportList, textWireValue, unwrapParens} from './ir.mjs';

export function unknownValueModule(sf, fn, allowTypeImports) {
  return sf.statements.every(statement => statement === fn
    || ts.isTypeAliasDeclaration(statement) || ts.isInterfaceDeclaration(statement)
    || isLocalExportList(statement)
    || allowTypeImports && ts.isImportDeclaration(statement) && statement.importClause?.isTypeOnly);
}

// Lower a closed native decision tree, not a serialization of an application
// callback. Each return is checked at its own source position; the kernel
// re-establishes the narrowing when it enters the corresponding path.
export function nativeTextFlow(sf, fn, checker, location, allowTypeImports = false) {
  const signature = checker.getSignatureFromDeclaration(fn);
  if (!signature || !(checker.getReturnTypeOfSignature(signature).flags & ts.TypeFlags.String)
      || !unknownValueModule(sf, fn, allowTypeImports) || fn.asteriskToken
      || fn.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) return;
  const domains = fn.parameters.map(param => checker.getTypeAtLocation(param.name).flags);
  if (domains.some(flags => !(flags & (ts.TypeFlags.Unknown | ts.TypeFlags.String)))) return;
  const params = fn.parameters.map((param, position) => ({
    name: param.name.text, publicName: param.name.text, position,
    domain: domains[position] & ts.TypeFlags.Unknown ? 'unknown' : 'string',
  }));
  const bindings = new Map(fn.parameters.map((param, i) => [checker.getSymbolAtLocation(param.name), params[i]]));
  const bound = node => node && ts.isIdentifier(node) && bindings.get(checker.getSymbolAtLocation(node));
  const literal = node => ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node);
  const native = node => {
    if (!ts.isIdentifier(node)) return false;
    const declarations = checker.getSymbolAtLocation(node)?.declarations;
    return declarations?.length > 0 && declarations.every(d => d.getSourceFile().hasNoDefaultLib);
  };
  const guardDepth = depth => {
    if (depth > 96) throw new Error('[NORM-ANALYSIS-INCOMPLETE] native text decision nesting budget exceeded at ' + location);
  };
  const value = (node, depth) => {
    guardDepth(depth);
    node = unwrapParens(node);
    if (literal(node)) return {kind: 'literal', value: textWireValue(node.text)};
    const input = bound(node);
    if (input && checker.getTypeAtLocation(node).flags & ts.TypeFlags.StringLike)
      return {kind: 'binding', name: input.name};
    if (ts.isCallExpression(node) && !node.questionDotToken
        && !node.typeArguments?.length && node.arguments.length === 1) {
      const callee = unwrapParens(node.expression);
      // Builtin String preserves an already proved native text value.
      // An assertion or an unproved input is never accepted in value().
      if (ts.isIdentifier(callee) && callee.text === 'String' && native(callee))
        return value(node.arguments[0], depth + 1);
    }
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken) {
      const left = value(node.left, depth + 1), right = value(node.right, depth + 1);
      if (left && right) return {kind: 'concat', left, right};
    }
  };
  const test = expr => {
    expr = unwrapParens(expr);
    if (!ts.isBinaryExpression(expr)) return;
    const operator = expr.operatorToken.kind;
    if (operator !== ts.SyntaxKind.EqualsEqualsEqualsToken
        && operator !== ts.SyntaxKind.ExclamationEqualsEqualsToken) return;
    let expression = unwrapParens(expr.left), token = unwrapParens(expr.right);
    if (literal(expression)) [expression, token] = [token, expression];
    if (!literal(token)) return;
    const tag = ts.isTypeOfExpression(expression);
    const input = bound(tag ? unwrapParens(expression.expression) : expression);
    if (!input || input.domain !== 'unknown' || (tag && token.text !== 'string')) return;
    return {
      condition: tag ? {kind: 'type', name: input.name}
        : {kind: 'equal', name: input.name, literal: textWireValue(token.text)},
      negate: operator === ts.SyntaxKind.ExclamationEqualsEqualsToken,
    };
  };
  const exceptions = {};
  const statements = node => ts.isBlock(node) ? [...node.statements] : [node];
  const sequence = (nodes, depth) => {
    guardDepth(depth);
    if (!nodes.length) return;
    const first = nodes[0];
    if (ts.isIfStatement(first)) {
      if (first.elseStatement && nodes.length !== 1) return;
      const checked = test(first.expression);
      if (!checked) return;
      const taken = sequence(statements(first.thenStatement), depth + 1);
      const other = sequence(first.elseStatement ? statements(first.elseStatement) : nodes.slice(1), depth + 1);
      if (!taken || !other) return;
      return {kind: 'if', condition: checked.condition,
        then: checked.negate ? other : taken, otherwise: checked.negate ? taken : other};
    }
    if (nodes.length !== 1 || !first.expression) return;
    if (ts.isReturnStatement(first)) {
      const result = value(first.expression, depth + 1);
      if (result) return {kind: 'return', value: result};
    }
    if (ts.isThrowStatement(first)) {
      const thrown = unwrapParens(first.expression);
      if (!ts.isNewExpression(thrown) || !native(thrown.expression) || thrown.typeArguments?.length
          || thrown.arguments?.length !== 1 || !literal(unwrapParens(thrown.arguments[0]))) return;
      const type = thrown.expression.text;
      if (type !== 'Error' && type !== 'TypeError') return;
      exceptions[type] = 'text';
      return {kind: 'throw', type, message: textWireValue(unwrapParens(thrown.arguments[0]).text)};
    }
  };
  const flow = sequence([...fn.body.statements], 0);
  if (flow?.kind === 'if') return {params, flow, exceptions};
}
