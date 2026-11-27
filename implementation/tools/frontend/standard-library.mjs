// The adapter owns this declaration. Source and dependencies cannot impersonate it.
import ts from 'typescript';

const specifier = '@normalizer/stdlib';
export const officialModulePath = '/__normalizer_builtin__/stdlib.d.ts';
const declarations = `
export function trim(value: string): string;
export function trim(value: string | null): string | null;
export function defaultIfBlank(value: string | null, fallback: string): string;
export function currentTimeMillis(): number;
`;
export function officialDeclaration() {
  return ts.createSourceFile(officialModulePath, declarations, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
}

export function isOfficialNamespaceImport(node) {
  if (!ts.isImportDeclaration(node) || !ts.isStringLiteral(node.moduleSpecifier)
      || node.moduleSpecifier.text !== specifier || node.attributes) return false;
  const clause = node.importClause;
  return !!clause && !clause.isTypeOnly && !clause.name
    && !!clause.namedBindings && ts.isNamespaceImport(clause.namedBindings);
}

export function standardCalls(checker, fail) {
  return (node, lower) => {
    if (!ts.isCallExpression(node) || !ts.isPropertyAccessExpression(node.expression)) return;
    const callee = node.expression;
    if (!ts.isIdentifier(callee.expression)) return;
    const symbol = checker.getSymbolAtLocation(callee.expression);
    const declaration = symbol?.valueDeclaration ?? symbol?.declarations?.[0];
    if (!declaration || !ts.isNamespaceImport(declaration)
        || !isOfficialNamespaceImport(declaration.parent.parent)) {
      if (callee.expression.text === 'N') fail('N call does not bind to the official @normalizer/stdlib namespace');
      return;
    }
    const signature = checker.getResolvedSignature(node)?.getDeclaration();
    if (signature?.getSourceFile().fileName !== officialModulePath
        || node.questionDotToken || callee.questionDotToken || node.typeArguments?.length
        || node.arguments.some(ts.isSpreadElement)) {
      fail('unsupported official @normalizer/stdlib member call');
    }
    return {op: 'official-call', name: callee.name.text, args: node.arguments.map(lower)};
  };
}
