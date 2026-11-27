// JavaScript/TypeScript syntax front-end. It lowers a deliberately explicit
// event subset to the language-neutral IR; it does not execute code or infer
// framework/template semantics.
import ts from 'typescript';

export function parseSourceModule(file, text, fail, kind = ts.ScriptKind.JS) {
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, kind);
  if (sf.parseDiagnostics.length) fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  return sf;
}

export const isLocalExportList = node => ts.isExportDeclaration(node) && !node.moduleSpecifier
  && !node.attributes && node.exportClause && ts.isNamedExports(node.exportClause);

export function definedModuleExport(sf, entry, fail) {
  const declarations = new Set();
  const functions = sf.statements.filter(ts.isFunctionDeclaration);
  for (const node of functions) {
    const modifiers = node.modifiers ?? [];
    if (!modifiers.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) continue;
    const exportedName = modifiers.some(modifier => modifier.kind === ts.SyntaxKind.DefaultKeyword)
      ? 'default' : node.name?.text;
    if (exportedName === entry) declarations.add(node);
  }
  for (const statement of sf.statements) {
    if (!isLocalExportList(statement) || statement.isTypeOnly) continue;
    for (const specifier of statement.exportClause.elements) {
      if (specifier.isTypeOnly || specifier.name.text !== entry) continue;
      const local = (specifier.propertyName ?? specifier.name).text;
      for (const node of functions) if (node.name?.text === local) declarations.add(node);
    }
  }
  const [fn] = declarations;
  if (declarations.size !== 1 || !fn.body) fail('expected one defined exported function: ' + entry);
  return fn;
}

const BINARY = new Map([
  [ts.SyntaxKind.PlusToken, '+'], [ts.SyntaxKind.MinusToken, '-'],
  [ts.SyntaxKind.GreaterThanToken, '>'], [ts.SyntaxKind.LessThanToken, '<'],
  [ts.SyntaxKind.GreaterThanEqualsToken, '>='], [ts.SyntaxKind.LessThanEqualsToken, '<='],
  [ts.SyntaxKind.EqualsEqualsEqualsToken, '==='], [ts.SyntaxKind.ExclamationEqualsEqualsToken, '!=='],
]);

export function unwrapParens(node) {
  while (ts.isParenthesizedExpression(node)) node = node.expression;
  return node;
}

function pathOf(node) {
  if (ts.isIdentifier(node)) return [node.text];
  if (node.kind === ts.SyntaxKind.ThisKeyword) return ['this'];
  const property = ts.isPropertyAccessExpression(node) ? node.name
    : ts.isElementAccessExpression(node) && node.argumentExpression && ts.isStringLiteral(node.argumentExpression)
      ? node.argumentExpression : undefined;
  if (property) {
    const parent = pathOf(node.expression);
    // A bracket string supplies one exact property key, even if it contains ".".
    if (parent) return [...parent, property.text];
  }
}

// Callers require an expression. Array holes remain explicit
// OmittedExpression nodes rather than fabricated value expressions.
function expr(node, sf, specialized) {
  const lower = child => expr(child, sf, specialized);
  node = unwrapParens(node);
  const result = specialized?.(node, lower);
  if (result !== undefined) return result;
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return { op: 'literal', type: 'string', value: node.text };
  if (ts.isNumericLiteral(node)) return { op: 'literal', type: 'number', value: Number(node.text) };
  if (ts.isPrefixUnaryExpression(node) && ts.isNumericLiteral(node.operand)
      && (node.operator === ts.SyntaxKind.MinusToken || node.operator === ts.SyntaxKind.PlusToken)) {
    const value = Number(node.operand.text);
    return { op: 'signed-number', value: node.operator === ts.SyntaxKind.MinusToken ? -value : value };
  }
  if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.ExclamationToken) {
    return { op: 'boolean-not', operand: lower(node.operand) };
  }
  if (node.kind === ts.SyntaxKind.TrueKeyword || node.kind === ts.SyntaxKind.FalseKeyword) {
    return { op: 'literal', type: 'boolean', value: node.kind === ts.SyntaxKind.TrueKeyword };
  }
  if (node.kind === ts.SyntaxKind.NullKeyword) return { op: 'literal', type: 'null', value: null };
  if (ts.isIdentifier(node)) return { op: 'var', name: node.text };
  if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
    const path = pathOf(node);
    if (path) return { op: 'read', target: path };
  }
  if (ts.isBinaryExpression(node)) {
    const operator = BINARY.get(node.operatorToken.kind);
    if (operator) return { op: 'binary', operator, left: lower(node.left), right: lower(node.right) };
  }
  if (ts.isArrayLiteralExpression(node)) return { op: 'array', items: node.elements.map(lower) };
  if (ts.isConditionalExpression(node)) return { op: 'select', condition: lower(node.condition), whenTrue: lower(node.whenTrue), whenFalse: lower(node.whenFalse) };
  return { op: 'opaque', text: node.getText(sf) };
}

export { expr as lowerExpression };
