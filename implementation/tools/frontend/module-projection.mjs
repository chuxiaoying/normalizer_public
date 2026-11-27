// Prepared module declarations supply the public identity of a component prop
// and the types of its instance state. Initial values describe field types only.
import ts from 'typescript';
import { definedModuleExport, isLocalExportList, lowerExpression, parseSourceModule } from './ir.mjs';
import { checkedTypeScript } from './typed-module.mjs';
import { localDirectives } from './local-directives.mjs';
import { isOfficialNamespaceImport } from './standard-library.mjs';

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
  return { source: `${originPath}#${page ? page + "." : ""}${name}`, fields: literalShape(decl.initializer) };
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

export function normalizerMarkers(node, reject = fail) {
  const markers = new Map();
  for (const tag of ts.getJSDocTags(node)) {
    if (tag.tagName.text !== 'N') continue;
    const raw = typeof tag.comment === 'string' ? tag.comment.trim() : '';
    const marker = /^(entryState|external|componentProp)(?:\s+([^\s]+))?$/.exec(raw);
    if (!marker || (marker[1] === 'componentProp') !== Boolean(marker[2])) {
      reject('unknown or malformed @N marker: ' + raw);
    }
    if (markers.has(marker[1])) reject('duplicate @N marker: ' + marker[1]);
    markers.set(marker[1], marker[2] ?? true);
  }
  return markers;
}

export function readModuleProjection(modulePath, source, entryName) {
  const sf = parseSourceModule(modulePath, source, fail, ts.ScriptKind.TS);
  const states = new Map();
  const fn = definedModuleExport(sf, entryName, fail);
  const usesStandard = sf.statements.some(isOfficialNamespaceImport);
  for (const statement of sf.statements) {
    const markers = normalizerMarkers(statement);
    if (!markers.has('entryState')) {
      // An ambient port declaration adds a type contract, not an initializer.
      if (markers.size === 1 && markers.has('external') && ts.isVariableStatement(statement)
          && statement.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.DeclareKeyword)) continue;
      if (markers.size && !(ts.isFunctionDeclaration(statement)
          && markers.size === 1 && markers.has('componentProp'))) {
        fail('component projection has a misplaced @N marker');
      }
      if (ts.isFunctionDeclaration(statement) || ts.isTypeAliasDeclaration(statement)
          || ts.isInterfaceDeclaration(statement) || isLocalExportList(statement)
          || isOfficialNamespaceImport(statement)) continue;
      fail('component projection cannot skip executable module initialization');
    }
    if (!ts.isVariableStatement(statement) || markers.size !== 1) fail('@N entryState only marks one state declaration');
    const decl = singleAnnotatedBinding(statement, '@N entryState must mark one named binding');
    const name = decl.name.text;
    states.set(name, { source: `${modulePath}#${name}`, fields: literalShape(decl.initializer) });
  }
  const statements = fn.body.statements;
  const returned = statements.at(-1);
  if (fn.modifiers?.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword)
      || fn.asteriskToken || fn.parameters.length
      || !returned || !ts.isReturnStatement(returned) || !returned.expression) {
    fail(`prepared component projection ${entryName} requires a synchronous exported function with a return expression and no parameters`);
  }
  const prop = normalizerMarkers(fn).get('componentProp');
  if (typeof prop !== 'string' || !/^[\w.-]+#[\w.-]+\.[\w-]+$/.test(prop)) fail(`missing or invalid @N componentProp on ${entryName}`);
  localDirectives(sf, sf).requireConsumed();
  checkedTypeScript(modulePath, sf, fail, {allowOfficial: usesStandard});
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
  return { function: `${modulePath}#${entryName}`, prop, states, locals, expression: lowerExpression(returned.expression, sf) };
}
