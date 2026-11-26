// JavaScript/TypeScript syntax front-end. It lowers a deliberately explicit
// event subset to the language-neutral IR; it does not execute code or infer
// framework/template semantics.
import ts from 'typescript';

export function parseSourceModule(file, text, fail, kind = ts.ScriptKind.JS) {
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, kind);
  if (sf.parseDiagnostics.length) fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  return sf;
}

export function definedModuleExport(sf, entry, fail) {
  const declarations = sf.statements.filter(node => ts.isFunctionDeclaration(node)
    && node.name?.text === entry && node.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword));
  if (declarations.length !== 1 || !declarations[0].body) fail('expected one defined exported function: ' + entry);
  return declarations[0];
}

const BINARY = new Map([
  [ts.SyntaxKind.PlusToken, '+'], [ts.SyntaxKind.MinusToken, '-'],
  [ts.SyntaxKind.GreaterThanToken, '>'], [ts.SyntaxKind.LessThanToken, '<'],
  [ts.SyntaxKind.EqualsEqualsEqualsToken, '==='],
]);

export function unwrapParens(node) {
  while (ts.isParenthesizedExpression(node)) node = node.expression;
  return node;
}

function pathOf(node) {
  if (ts.isIdentifier(node)) return [node.text];
  const property = ts.isPropertyAccessExpression(node) ? node.name
    : ts.isElementAccessExpression(node) && node.argumentExpression && ts.isStringLiteral(node.argumentExpression)
      ? node.argumentExpression : undefined;
  if (!property) return;
  const parent = pathOf(node.expression);
  // A bracket string supplies one exact property key, even if it contains ".".
  if (parent) return [...parent, property.text];
}

// Callers require an expression. Array holes remain explicit
// OmittedExpression nodes rather than fabricated value expressions.
function expr(node, sf) {
  node = unwrapParens(node);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return { op: 'literal', type: 'string', value: node.text };
  if (ts.isNumericLiteral(node)) return { op: 'literal', type: 'number', value: Number(node.text) };
  if (node.kind === ts.SyntaxKind.TrueKeyword || node.kind === ts.SyntaxKind.FalseKeyword) return { op: 'literal', type: 'boolean', value: node.kind === ts.SyntaxKind.TrueKeyword };
  if (node.kind === ts.SyntaxKind.NullKeyword) return { op: 'literal', type: 'null', value: null };
  if (ts.isIdentifier(node)) return { op: 'var', name: node.text };
  if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
    const path = pathOf(node);
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
