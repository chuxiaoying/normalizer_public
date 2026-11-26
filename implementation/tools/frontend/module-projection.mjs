// Prepared module declarations supply the public identity of a component prop
// and the types of its instance state. Initial values describe field types only.
import { basename, extname } from 'node:path';
import ts from 'typescript';
import { lowerExpression, parseSourceModule } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';

function fail(detail) {
  throw new Error(`SCENARIO_MODULE_INPUT: ${detail}`);
}

const stateScalarTypes = new Map([
  [ts.SyntaxKind.NumericLiteral, 'number'], [ts.SyntaxKind.StringLiteral, 'text'],
  [ts.SyntaxKind.TrueKeyword, 'bool'], [ts.SyntaxKind.FalseKeyword, 'bool'],
]);

export function singleAnnotatedBinding(statement, message, reject = fail) {
  const declarations = statement.declarationList.declarations;
  if (declarations.length !== 1 || !ts.isIdentifier(declarations[0].name)) reject(message);
  return declarations[0];
}

export function stateBinding(decl, name, originPath, page) {
  return { source: `${originPath}#${page}.${name}`, fields: literalShape(decl.initializer) };
}

function literalShape(node) {
  if (node && ts.isObjectLiteralExpression(node)) {
    return Object.fromEntries(node.properties.map(property => {
      if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)) {
        fail('@entryState requires a plain data record with named properties');
      }
      return [property.name.text, literalShape(property.initializer)];
    }));
  }
  return stateScalarTypes.get(node?.kind) ?? fail('state field requires a plain scalar or record initializer');
}

export function readModuleProjection(modulePath, source, entryName, originPath) {
  const sf = parseSourceModule(modulePath, source, fail);
  const states = new Map();
  const page = basename(originPath, extname(originPath));
  for (const statement of sf.statements) {
    if (!ts.isVariableStatement(statement) || !ts.getJSDocTags(statement).some(tag => tag.tagName.text === 'entryState')) continue;
    const decl = singleAnnotatedBinding(statement, '@entryState must mark one named binding');
    if (ts.getJSDocType(statement) || ts.getJSDocType(decl)) fail('@entryState projection requires a plain literal shape');
    const name = decl.name.text;
    states.set(name, stateBinding(decl, name, originPath, page));
  }
  const fn = sf.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === entryName);
  const statements = fn?.body?.statements;
  const returned = statements?.at(-1);
  if (!fn?.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)
      || fn.modifiers.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword)
      || fn.parameters.length || !returned || !ts.isReturnStatement(returned) || !returned.expression) {
    fail(`prepared component projection ${entryName} requires a synchronous exported function with a return expression and no parameters`);
  }
  const prop = ts.getJSDocTags(fn).find(tag => tag.tagName.text === 'componentProp')?.comment;
  if (typeof prop !== 'string' || !/^[\w.-]+#[\w.-]+\.[\w-]+$/.test(prop)) fail(`missing or invalid @componentProp on ${entryName}`);
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
