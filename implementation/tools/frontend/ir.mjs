// JavaScript/TypeScript syntax front-end. It lowers a deliberately explicit
// event subset to the language-neutral IR; it does not execute code or infer
// framework/template semantics.
import ts from 'typescript';

const BINARY = new Map([
  [ts.SyntaxKind.PlusToken, '+'], [ts.SyntaxKind.MinusToken, '-'],
  [ts.SyntaxKind.GreaterThanToken, '>'], [ts.SyntaxKind.LessThanToken, '<'],
  [ts.SyntaxKind.EqualsEqualsEqualsToken, '==='],
]);

function pathOf(node, sf) {
  if (ts.isIdentifier(node)) return node.text;
  if (node.kind === ts.SyntaxKind.ThisKeyword) return 'this';
  if (ts.isPropertyAccessExpression(node)) {
    const parent = pathOf(node.expression, sf);
    return parent ? `${parent}.${node.name.text}` : null;
  }
  if (ts.isElementAccessExpression(node) && node.argumentExpression && ts.isStringLiteral(node.argumentExpression)) {
    const parent = pathOf(node.expression, sf);
    return parent ? `${parent}.${node.argumentExpression.text}` : null;
  }
  return null;
}

function expr(node, sf) {
  if (!node) return { op: 'undefined' };
  if (ts.isParenthesizedExpression(node)) return expr(node.expression, sf);
  // These are compile-time annotations, not runtime wrappers. Keeping them
  // opaque would hide calls underneath `await nativeRpc(...) as Reply`.
  if (ts.isAsExpression(node) || ts.isTypeAssertionExpression(node) || ts.isNonNullExpression(node) || ts.isSatisfiesExpression(node)) return expr(node.expression, sf);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return { op: 'literal', type: 'string', value: node.text };
  if (ts.isNumericLiteral(node)) return { op: 'literal', type: 'number', value: Number(node.text) };
  if (node.kind === ts.SyntaxKind.TrueKeyword || node.kind === ts.SyntaxKind.FalseKeyword) return { op: 'literal', type: 'boolean', value: node.kind === ts.SyntaxKind.TrueKeyword };
  if (node.kind === ts.SyntaxKind.NullKeyword) return { op: 'literal', type: 'null', value: null };
  if (ts.isIdentifier(node)) return { op: 'var', name: node.text };
  if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
    const path = pathOf(node, sf);
    if (path) return { op: 'read', target: path };
  }
  if (ts.isBinaryExpression(node)) {
    const operator = BINARY.get(node.operatorToken.kind);
    if (operator) return { op: 'binary', operator, left: expr(node.left, sf), right: expr(node.right, sf) };
  }
  if (ts.isArrayLiteralExpression(node)) return { op: 'array', items: node.elements.map(item => expr(item, sf)) };
  if (ts.isConditionalExpression(node)) return { op: 'select', condition: expr(node.condition, sf), whenTrue: expr(node.whenTrue, sf), whenFalse: expr(node.whenFalse, sf) };
  return { op: 'opaque', text: node.getText(sf) };
}

export { expr as lowerExpression };
