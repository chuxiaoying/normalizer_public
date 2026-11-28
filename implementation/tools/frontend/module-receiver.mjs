// A finite module-startup proof for explicitly exported literal receivers.
// Preserve which literal provided `this`; never detach the method or infer
// the receiver from a matching property name on a different declaration.
import ts from 'typescript';
import { unwrapParens } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';

function objectLiteral(declaration) {
  const init = declaration.initializer && unwrapParens(declaration.initializer);
  if (!init || !ts.isAsExpression(init) || init.type.getText() !== 'const') return;
  const value = unwrapParens(init.expression);
  return ts.isObjectLiteralExpression(value) ? value : undefined;
}

export function closedReceiver(sf, entry, fail, {linked = false} = {}) {
  localDirectives(sf, sf).requireConsumed(line =>
    fail(`line ${line}: closed receiver modules do not admit @normalizer-ir directives`));
  const names = entry.split('.');
  if (names.length !== 2) fail('object entry needs one exported receiver and one method: ' + entry);
  const [receiver, member] = names;
  let chosen;
  const assembly = [];
  for (const declaration of sf.statements) {
    if (linked && ts.isImportDeclaration(declaration)) continue; // Already resolved in the checked graph.
    if (ts.isFunctionDeclaration(declaration) || ts.isTypeAliasDeclaration(declaration)
        || ts.isInterfaceDeclaration(declaration)) continue;
    const statements = ts.isVariableStatement(declaration) &&
      declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword) &&
      declaration.declarationList.flags & ts.NodeFlags.Const && declaration.declarationList.declarations;
    if (!statements || statements.length !== 1) fail('object entry requires closed literal module assembly');
    const [variable] = statements;
    const literal = ts.isIdentifier(variable.name) && objectLiteral(variable);
    if (!literal) fail('object entry requires an exported const receiver with literal as const assembly');
    const fields = new Map(), methods = new Map(), used = new Set();
    for (const property of literal.properties) {
      // An identifier and its quoted spelling name the same static member.
      const name = property.name && (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name))
        ? property.name.text : undefined;
      if (name === undefined || name === '__proto__' || used.has(name)) {
        fail('object receiver requires unique plain field and method names');
      }
      used.add(name);
      if (ts.isMethodDeclaration(property) && property.body && !property.asteriskToken
          && !property.typeParameters?.length && !property.modifiers?.length) {
        methods.set(name, property);
      } else if (ts.isPropertyAssignment(property)) {
        const initializer = unwrapParens(property.initializer);
        fields.set(name, initializer);
        assembly.push(initializer);
      } else fail('object receiver cannot use computed access, spreading, or a dynamic member: ' + name);
    }
    if (variable.name.text === receiver) chosen = {fields, methods};
  }
  if (!chosen) fail('entry receiver is not an exported const object: ' + receiver);
  const fn = chosen.methods.get(member);
  if (!fn) fail('entry method is not declared on its exported receiver: ' + entry);
  return {fn, fields: chosen.fields, assembly};
}
