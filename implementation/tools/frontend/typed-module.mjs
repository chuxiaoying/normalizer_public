// The typed standalone syntactic and type-checking gate shared by .ts shapes.
import ts from 'typescript';
import {basename, dirname} from 'node:path';
import {definedModuleExport, parseSourceModule} from './ir.mjs';
import {localDirectives} from './local-directives.mjs';
import {isOfficialModuleImport, isOfficialNamespaceImport, invalidOfficialImport, officialModulePath, officialDeclaration} from './standard-library.mjs';

export const failTypedModule = message => { throw new Error('[NORM-TS-MODULE] ' + message); };
export const typedIdentifier = x => typeof x === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(x) && x !== 'undefined';

export function typedModuleAst(source, physicalPath, entry) {
  const sf = parseSourceModule(physicalPath, source, failTypedModule, ts.ScriptKind.TS);
  const fn = definedModuleExport(sf, entry, failTypedModule);
  localDirectives(sf, sf).requireConsumed();
  return {sf, fn};
}

export function typedParameterNames(fn) {
  return fn.parameters.map(param => param.name.text);
}

export function isCompilerLibrary(file, directory) {
  return dirname(file) === directory && /^lib(?:[.][a-z0-9-]+)*[.]d[.]ts$/.test(basename(file));
}

export function typeScriptProgram(physicalPath, sf, fail = failTypedModule, {allowOfficial = false} = {}) {
  if (sf.checkJsDirective?.enabled === false || sf.commentDirectives?.length) {
    fail('TypeScript checking directives cannot disable or suppress the strict preflight');
  }
  if (sf.referencedFiles.length || sf.typeReferenceDirectives.length
      || sf.libReferenceDirectives.length || sf.hasNoDefaultLib) {
    fail('compiler reference directives are not scoped module imports');
  }
  let hasImport = false, hasOfficial = false;
  const inspect = node => {
    if (node.kind === ts.SyntaxKind.AnyKeyword) fail('explicit any is not an admitted analysis type');
    if (isOfficialModuleImport(node) && !isOfficialNamespaceImport(node)) fail('[NORM-IMPORT] ' + invalidOfficialImport);
    if (allowOfficial && isOfficialNamespaceImport(node)) {
      hasOfficial = true;
      return;
    }
    if (ts.isImportTypeNode(node) || node.kind === ts.SyntaxKind.ImportKeyword) hasImport = true;
    ts.forEachChild(node, inspect);
  };
  inspect(sf);
  if (hasImport) fail('[NORM-IMPORT] typed module cannot import outside declarations');
  const options = {strict: true, noEmit: true, noResolve: true, types: [], target: ts.ScriptTarget.ES2022};
  const host = ts.createCompilerHost(options);
  const readBuiltin = host.getSourceFile.bind(host);
  const library = dirname(host.getDefaultLibFileName(options));
  host.getSourceFile = (name, ...args) => {
    if (name === physicalPath) return sf;
    if (hasOfficial && name === officialModulePath) return officialDeclaration();
    if (isCompilerLibrary(name, library)) return readBuiltin(name, ...args);
  };
  if (hasOfficial) host.resolveModuleNames = names => names.map(name => name === '@normalizer/stdlib'
    ? {resolvedFileName: officialModulePath, extension: ts.Extension.Dts, isExternalLibraryImport: true}
    : undefined);
  const checked = ts.createProgram(hasOfficial ? [physicalPath, officialModulePath] : [physicalPath], options, host);
  const errors = [...checked.getSyntacticDiagnostics(), ...checked.getSemanticDiagnostics()];
  return { checker: checked.getTypeChecker(), errors };
}

export function assertTypeScript(sf, checker, errors, fail = failTypedModule) {
  if (errors.length) fail(ts.flattenDiagnosticMessageText(errors[0].messageText, '\n'));
  assertNoResolvedAny(sf, checker, fail);
  assertClosedPlatformReferences(sf, checker, fail);
}

// The compiler loads library declarations for typing, not for implicit
// access to browser, Node or standard-library operations. Check the binding,
// so a project variable named fetch does not become a host reference.
const nativeTypes = new Set([
  'Array', 'ReadonlyArray', 'Map', 'ReadonlyMap', 'Promise', 'Record', 'Partial',
  'Readonly', 'Pick', 'Omit', 'Exclude', 'Extract', 'NonNullable',
  'ReturnType', 'Parameters', 'Awaited', 'Error', 'TypeError',
]);
const nativeValues = new Set(['Map', 'Error', 'TypeError', 'String', 'Boolean', 'undefined']);
const nativeMembers = new Set([
  'Array.length', 'ReadonlyArray.length', 'String.length', 'Error.message', 'Promise.then',
]);

function assertClosedPlatformReferences(sf, checker, fail) {
  const failAt = (node, detail) => {
    const {line, character} = sf.getLineAndCharacterOfPosition(node.getStart(sf));
    fail(`[NORM-IMPORT] ${sf.fileName}:${line + 1}:${character + 1}: ${detail}`);
  };
  const isLibraryDeclaration = declaration => {
    const source = declaration.getSourceFile();
    return source.isDeclarationFile && /^lib(?:[.][a-z0-9-]+)*[.]d[.]ts$/.test(basename(source.fileName));
  };
  const isLibraryGlobal = declaration => {
    if (!isLibraryDeclaration(declaration)) return false;
    const statement = ts.isVariableDeclaration(declaration) ? declaration.parent.parent : declaration;
    return ts.isSourceFile(statement.parent);
  };
  const checkMember = (node, member) => {
    for (const declaration of member?.declarations ?? []) {
      if (!isLibraryDeclaration(declaration)) continue;
      const owner = declaration.parent.name;
      if (!owner || !ts.isIdentifier(owner) || !nativeMembers.has(owner.text + '.' + member.getName())) {
        failAt(node, 'platform member ' + node.getText(sf) + ' has no admitted source contract');
      }
    }
  };
  const visit = node => {
    if (ts.isImportTypeNode(node) || (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword)) {
      failAt(node, 'only explicit project-relative and official namespace imports are admitted');
    }
    if (ts.isIdentifier(node)) {
      const parent = node.parent;
      if (!(ts.isPropertyAccessExpression(parent) && parent.name === node)
          && !(ts.isQualifiedName(parent) && parent.right === node)) {
        let symbol = checker.getSymbolAtLocation(node);
        if (symbol?.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
        const name = symbol?.getName();
        const global = symbol?.declarations?.some(isLibraryGlobal)
          || (name === 'globalThis' && !symbol?.declarations?.length);
        if (global) {
          const typeOnly = ts.isPartOfTypeNode(node) && !ts.isTypeQueryNode(parent);
          if (!(typeOnly ? nativeTypes : nativeValues).has(name)) {
            failAt(node, 'platform binding ' + name + ' is outside the source allowlist');
          }
          // Admitting built-in constructors/value conversions does not admit
          // arbitrary namespace members or aliases to host prototype methods.
          if (!typeOnly && (ts.isPropertyAccessExpression(parent) || ts.isElementAccessExpression(parent))
              && parent.expression === node) {
            failAt(parent, 'platform member ' + parent.getText(sf) + ' has no admitted source contract');
          }
        }
      }
    }
    ts.forEachChild(node, visit);
    if (ts.isPropertyAccessExpression(node)) {
      checkMember(node, checker.getSymbolAtLocation(node.name));
    } else if (ts.isElementAccessExpression(node)) {
      const type = checker.getTypeAtLocation(node.argumentExpression);
      const receiver = checker.getApparentType(checker.getNonNullableType(checker.getTypeAtLocation(node.expression)));
      for (const key of type.isUnion() ? type.types : [type]) {
        if (key.isStringLiteral()) checkMember(node, checker.getPropertyOfType(receiver, key.value));
      }
    }
  };
  visit(sf);
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
    if (!(type.flags & ts.TypeFlags.Object)) return false;
    if (type.objectFlags & ts.ObjectFlags.Reference && checker.getTypeArguments(type).some(hasAny)) return true;

    // A source-defined data field's resolved type is part of the supplied
    // value contract, even if the current method does not read that field.
    // Don't sweep unused methods on compiler-library prototypes.
    const sourceDeclaration = declarations => declarations?.find(declaration => !declaration.getSourceFile().isDeclarationFile);
    for (const property of checker.getPropertiesOfType(type)) {
      if (!(property.flags & (ts.SymbolFlags.Property | ts.SymbolFlags.GetAccessor | ts.SymbolFlags.SetAccessor))) continue;
      const declaration = sourceDeclaration(property.declarations);
      if (declaration && hasAny(checker.getTypeOfSymbolAtLocation(property, declaration))) return true;
    }
    return checker.getIndexInfosOfType(type).some(index =>
      index.declaration && !index.declaration.getSourceFile().isDeclarationFile && hasAny(index.type));
  };
  const inspect = node => {
    const constAssertion = ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName)
      && node.typeName.text === 'const' && (ts.isAsExpression(node.parent) || ts.isTypeAssertionExpression(node.parent));
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

export function checkedTypeScript(physicalPath, sf, fail = failTypedModule, options) {
  const { checker, errors } = typeScriptProgram(physicalPath, sf, fail, options);
  assertTypeScript(sf, checker, errors, fail);
  return checker;
}
