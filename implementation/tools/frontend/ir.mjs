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
  [ts.SyntaxKind.PlusToken, '+'], [ts.SyntaxKind.MinusToken, '-'], [ts.SyntaxKind.AsteriskToken, '*'],
  [ts.SyntaxKind.GreaterThanToken, '>'], [ts.SyntaxKind.LessThanToken, '<'],
  [ts.SyntaxKind.GreaterThanEqualsToken, '>='], [ts.SyntaxKind.LessThanEqualsToken, '<='],
  [ts.SyntaxKind.EqualsEqualsEqualsToken, '==='], [ts.SyntaxKind.ExclamationEqualsEqualsToken, '!=='],
  [ts.SyntaxKind.AmpersandAmpersandToken, '&&'], [ts.SyntaxKind.BarBarToken, '||'],
]);

export function unwrapParens(node) {
  while (ts.isParenthesizedExpression(node)) node = node.expression;
  return node;
}

// The parser value of a NumericLiteral is already converted to IEEE-754.
// Read the token spelling instead; a decimal fraction cannot masquerade
// as an integer after host rounding.
export function exactIntegerLiteral(node, sf) {
  node = unwrapParens(node);
  let negative = false;
  if (ts.isPrefixUnaryExpression(node)
      && [ts.SyntaxKind.MinusToken, ts.SyntaxKind.PlusToken].includes(node.operator)) {
    negative = node.operator === ts.SyntaxKind.MinusToken;
    node = unwrapParens(node.operand);
  }
  if (!ts.isNumericLiteral(node)) return undefined;
  const text = node.getText(sf).replaceAll('_', '');
  let value;
  if (/^0(?:[xX][\da-fA-F]+|[oO][0-7]+|[bB][01]+)$/.test(text)) {
    value = BigInt(text);
  } else {
    const parts = /^(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(text);
    if (!parts) return undefined;
    const [, whole, fraction = '', exponent = '0'] = parts;
    const digits = (whole + fraction).replace(/^0+/, '');
    if (!digits) return '0';
    const radix = BigInt(exponent) - BigInt(fraction.length);
    if (radix < 0n) {
      const trailingZeroes = digits.length - digits.replace(/0+$/, '').length;
      const trimmed = -radix;
      if (trimmed > BigInt(trailingZeroes)) return undefined;
      value = BigInt(digits.slice(0, digits.length - Number(trimmed)));
    } else {
      if (radix > 1_000_000n) throw new Error('[NORM-ANALYSIS-INCOMPLETE] exact integer expansion budget exceeded');
      value = BigInt(digits + '0'.repeat(Number(radix)));
    }
  }
  return (negative ? -value : value).toString();
}

// JSON text consumed by the Rust bridge must be Unicode scalar text.
// Carry ill-formed source strings as their exact UTF-16 code units instead.
export function textWireValue(text) {
  for (let index = 0; index < text.length; index++) {
    const unit = text.charCodeAt(index);
    if (unit < 0xD800 || unit > 0xDFFF) continue;
    const next = text.charCodeAt(index + 1);
    if (unit < 0xDC00 && next >= 0xDC00 && next <= 0xDFFF) {
      index++;
      continue;
    }
    return Array.from({length: text.length}, (_, offset) => text.charCodeAt(offset));
  }
  return text;
}

function pathOf(node) {
  if (ts.isIdentifier(node)) return [node.text];
  if (node.kind === ts.SyntaxKind.ThisKeyword) return ['this'];
  if (node.questionDotToken) return undefined; // Optional access is not a plain path read.
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
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return { op: 'literal', type: 'string', value: textWireValue(node.text) };
  const number = exactIntegerLiteral(node, sf);
  if (number !== undefined) return { op: 'literal', type: 'number', value: number };
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
