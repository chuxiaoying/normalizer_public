// Prepared module declarations supply the public identity of a component prop
// and the types of its instance state. Initial values describe field types only.
import { basename, extname } from 'node:path';
import ts from 'typescript';
import { lowerExpression } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';

function fail(detail) {
  throw new Error(`SCENARIO_MODULE_INPUT: ${detail}`);
}

export function literalShape(node) {
  if (node && ts.isObjectLiteralExpression(node)) {
    return Object.fromEntries(node.properties.map(property => {
      if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)) {
        fail('@entryState requires a plain data record with named properties');
      }
      return [property.name.text, literalShape(property.initializer)];
    }));
  }
  if (node && ts.isNumericLiteral(node)) return 'number';
  if (node && ts.isStringLiteral(node)) return 'text';
  if (node && [ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword].includes(node.kind)) return 'bool';
  fail('state field requires a plain scalar or record initializer');
}

export function readModuleProjection(modulePath, source, entryName, originPath) {
  const sf = ts.createSourceFile(modulePath, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  if (sf.parseDiagnostics.length) fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  const states = new Map();
  const page = basename(originPath, extname(originPath));
  for (const statement of sf.statements) {
    if (!ts.isVariableStatement(statement) || !ts.getJSDocTags(statement).some(tag => tag.tagName.text === 'entryState')) continue;
    const declarations = statement.declarationList.declarations;
    if (declarations.length !== 1 || !ts.isIdentifier(declarations[0].name)) fail('@entryState must mark one named binding');
    const name = declarations[0].name.text;
    const declared = ts.getJSDocType(statement) || ts.getJSDocType(declarations[0]);
    if (declared) fail('@entryState projection requires a plain literal shape');
    const fields = literalShape(declarations[0].initializer);
    states.set(name, { source: `${originPath}#${page}.${name}`, fields });
  }
  const fn = sf.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === entryName);
  if (!fn || !fn.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) {
    fail(`prepared module must export function ${entryName}`);
  }
  if (fn.modifiers.some(modifier => modifier.kind === ts.SyntaxKind.AsyncKeyword)) fail(`async entry ${entryName} requires lowering await, completion and time-specific host input reads`);
  const prop = ts.getJSDocTags(fn).find(tag => tag.tagName.text === 'componentProp')?.comment;
  if (typeof prop !== 'string' || !/^[\w.-]+#[\w.-]+\.[\w-]+$/.test(prop)) fail(`missing or invalid @componentProp on ${entryName}`);
  const statements = fn.body?.statements ?? [];
  const returned = statements.at(-1);
  if (fn.parameters.length || !returned || !ts.isReturnStatement(returned) || !returned.expression) {
    fail(`synchronous component projection ${entryName} requires a return expression and no parameters`);
  }
  localDirectives(sf, fn.body).requireConsumed();
  const locals = [];
  for (const st of statements.slice(0, -1)) {
    if (!ts.isVariableStatement(st) || !(st.declarationList.flags & ts.NodeFlags.Const)) {
      fail('only const bindings may precede a component projection return');
    }
    for (const declaration of st.declarationList.declarations) {
      if (!ts.isIdentifier(declaration.name) || !declaration.initializer) fail('projection const requires a named initializer');
      locals.push({name: declaration.name.text, value: lowerExpression(declaration.initializer, sf)});
    }
  }
  return { function: `${originPath}#${prop}`, states, locals, expression: lowerExpression(returned.expression, sf) };
}
