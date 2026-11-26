// The typed standalone syntactic and type-checking gate shared by .ts shapes.
import ts from 'typescript';
import {definedModuleExport, parseSourceModule} from './ir.mjs';
import {localDirectives} from './local-directives.mjs';

export const failTypedModule = message => { throw new Error('[NORM-TS-MODULE] ' + message); };
export const typedIdentifier = x => typeof x === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(x) && x !== 'undefined';

export function typedModuleAst(source, physicalPath, entry) {
  const sf = parseSourceModule(physicalPath, source, failTypedModule, ts.ScriptKind.TS);
  const fn = definedModuleExport(sf, entry, failTypedModule);
  for (const declaration of sf.statements) {
    if (ts.isFunctionDeclaration(declaration) && declaration.body) localDirectives(sf, declaration.body).requireConsumed();
  }
  return {sf, fn};
}

export function typedParameterNames(fn) {
  return fn.parameters.map(param => param.name.text);
}

export function typeScriptProgram(physicalPath, sf, fail = failTypedModule) {
  let hasImport = false;
  const inspect = node => {
    if (node.kind === ts.SyntaxKind.AnyKeyword) fail('explicit any is not an admitted analysis type');
    if (ts.isImportTypeNode(node) || node.kind === ts.SyntaxKind.ImportKeyword) hasImport = true;
    ts.forEachChild(node, inspect);
  };
  inspect(sf);
  if (hasImport) fail('typed module cannot import outside declarations');
  const options = {strict: true, noEmit: true, noResolve: true, types: [], target: ts.ScriptTarget.ES2022};
  const host = ts.createCompilerHost(options);
  const readSource = host.getSourceFile.bind(host);
  host.getSourceFile = (name, ...args) => name === physicalPath ? sf : readSource(name, ...args);
  const checked = ts.createProgram([physicalPath], options, host);
  const errors = [...checked.getSyntacticDiagnostics(), ...checked.getSemanticDiagnostics()];
  return { checker: checked.getTypeChecker(), errors };
}

export function assertTypeScript(errors, fail = failTypedModule) {
  if (errors.length) fail(ts.flattenDiagnosticMessageText(errors[0].messageText, '\n'));
}

// strict/noImplicitAny alone permits dynamic types originating in the
// standard library (e.g. JSON.parse or the default of Array<T>).
function assertNoResolvedAny(sf, checker, fail) {
  const visited = new Set();
  const hasAny = type => {
    if (type.flags & ts.TypeFlags.Any) return true;
    if (visited.has(type)) return false;
    visited.add(type);
    if (type.isUnionOrIntersection() && type.types.some(hasAny)) return true;
    if (type.aliasTypeArguments?.some(hasAny)) return true;
    return type.flags & ts.TypeFlags.Object && type.objectFlags & ts.ObjectFlags.Reference
      && checker.getTypeArguments(type).some(hasAny);
  };
  const inspect = node => {
    const constAssertion = ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName)
      && node.typeName.text === 'const' && ts.isAsExpression(node.parent);
    if (constAssertion) return;
    const binding = (ts.isVariableDeclaration(node) || ts.isParameter(node) || ts.isBindingElement(node))
      && ts.isIdentifier(node.name) ? node.name : undefined;
    const observed = binding ?? (ts.isExpressionNode(node) || ts.isTypeNode(node) ? node : undefined);
    // The tuple/object binding pattern itself is syntax, not a typed value.
    // Its individual binding elements and initializer are visited separately.
    if (observed && hasAny(checker.getTypeAtLocation(observed))) {
      const {line, character} = sf.getLineAndCharacterOfPosition(observed.getStart(sf));
      fail(`resolved any is not an admitted analysis type at ${line + 1}:${character + 1}`);
    }
    ts.forEachChild(node, inspect);
  };
  inspect(sf);
}

export function checkedTypeScript(physicalPath, sf, fail = failTypedModule, {rejectAny = false} = {}) {
  if (rejectAny && (sf.checkJsDirective?.enabled === false || sf.commentDirectives?.length)) {
    fail('TypeScript checking directives cannot disable or suppress the strict preflight');
  }
  const { checker, errors } = typeScriptProgram(physicalPath, sf, fail);
  assertTypeScript(errors, fail);
  if (rejectAny) assertNoResolvedAny(sf, checker, fail);
  return checker;
}
