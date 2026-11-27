// Compile only a proven graph of project-owned, executable TypeScript modules.
// The host does not resolve arbitrary packages, run decorators/initializers,
// or invent declaration stubs for a missing import.
import { readFileSync } from 'node:fs';
import { basename, dirname, posix } from 'node:path';
import ts from 'typescript';
import { parseSourceModule } from './ir.mjs';
import { assertTypeScript } from './typed-module.mjs';
import { localDirectives } from './local-directives.mjs';
import {isOfficialNamespaceImport, officialModulePath, officialDeclaration} from './standard-library.mjs';

const relative = specifier => specifier.startsWith('./') || specifier.startsWith('../');
const ordinaryDeclaration = node => ts.isImportDeclaration(node) || ts.isFunctionDeclaration(node)
  || ts.isTypeAliasDeclaration(node) || ts.isInterfaceDeclaration(node);

export function hasProjectImports(source, file) {
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  return sf.statements.some(st => ts.isImportDeclaration(st) && ts.isStringLiteral(st.moduleSpecifier)
    && relative(st.moduleSpecifier.text));
}

export function checkedProjectModules(entry, projectSource, fail, {rootReceiver = false} = {}) {
  const modules = new Map();
  const paths = new Map();
  const visiting = new Set();
  let hasOfficial = false;
  const diagnostic = (record, node, reason) => {
    const start = record.ast.getLineAndCharacterOfPosition(node.getStart(record.ast));
    fail(`${record.name}:${start.line + 1}:${start.character + 1}: ${reason}`);
  };

  function add(input) {
    if (visiting.has(input.declared)) fail('cyclic project module dependency: ' + input.name);
    if (modules.has(input.declared)) return;
    if (paths.has(input.path)) fail('ambiguous project module alias: ' + input.name);
    if (modules.size >= 128) throw new Error('[NORM-ANALYSIS-INCOMPLETE] project module graph budget exceeded at ' + input.name);
    paths.set(input.path, input.name);
    const source = readFileSync(input.path, 'utf8');
    const ast = parseSourceModule(input.declared, source, message => fail(input.name + ': ' + message), ts.ScriptKind.TS);
    localDirectives(ast, ast).requireConsumed(line =>
      fail(`${input.name}:${line}: linked scalar modules do not admit @normalizer-ir directives`));
    const record = {...input, ast, dependencies: new Map()};
    modules.set(input.declared, record);
    visiting.add(input.declared);
    if (ast.checkJsDirective?.enabled === false || ast.commentDirectives?.length) {
      fail(input.name + ': TypeScript checking directives cannot disable or suppress the strict preflight');
    }
    if (ast.referencedFiles.length || ast.typeReferenceDirectives.length || ast.libReferenceDirectives.length || ast.hasNoDefaultLib) {
      fail(input.name + ': compiler reference directives are not scoped module imports');
    }
    for (const st of ast.statements) {
      if (!ordinaryDeclaration(st) && !(rootReceiver && input.declared === entry.declared && ts.isVariableStatement(st))) diagnostic(record, st, 'linked modules require closed declarations without executable initialization');
      if (!ts.isImportDeclaration(st)) continue;
      const specifier = ts.isStringLiteral(st.moduleSpecifier) && st.moduleSpecifier.text;
      if (isOfficialNamespaceImport(st)) {
        record.dependencies.set(specifier, officialModulePath);
        hasOfficial = true;
        continue;
      }
      if (!specifier || !relative(specifier) || /[\\:%?#\0]/.test(specifier)) {
        diagnostic(record, st, 'linked module import must name an executable relative project source');
      }
      const clause = st.importClause;
      const named = clause?.namedBindings;
      if (!clause || (named && !ts.isNamedImports(named))
          || (!clause.name && (!named || !named.elements.length))) {
        diagnostic(record, st, 'linked modules require explicit named or default bindings, not side-effect imports');
      }
      const sourcePath = posix.normalize(posix.join(posix.dirname(input.name), specifier));
      if (sourcePath === '..' || sourcePath.startsWith('../')) diagnostic(record, st, 'relative import escapes the project root');
      const targetPath = sourcePath.replace(/\.js$/, '.ts');
      if (!targetPath.endsWith('.ts') || targetPath.endsWith('.d.ts')) {
        diagnostic(record, st, 'relative import must resolve to an executable .ts source');
      }
      let dependency;
      try { dependency = projectSource(targetPath); }
      catch (error) {
        diagnostic(record, st, error.code === 'ENOENT' ? 'relative source ' + specifier + ' is missing'
          : 'relative source ' + specifier + ': ' + error.message);
      }
      if (!dependency.path.endsWith('.ts') || dependency.path.endsWith('.d.ts')) {
        diagnostic(record, st, 'relative import must resolve to an executable .ts source');
      }
      record.dependencies.set(specifier, dependency.declared);
      add(dependency);
    }
    visiting.delete(input.declared);
  }

  add(entry);
  const options = {strict: true, noEmit: true, types: [], target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext, allowImportingTsExtensions: true, allowSyntheticDefaultImports: false};
  const host = ts.createCompilerHost(options);
  const builtinSource = host.getSourceFile.bind(host);
  const library = dirname(host.getDefaultLibFileName(options));
  host.getSourceFile = (file, ...rest) => {
    if (hasOfficial && file === officialModulePath) return officialDeclaration();
    const known = modules.get(file);
    if (known) return known.ast;
    if (dirname(file) === library && /^lib(?:[.][a-z0-9-]+)*[.]d[.]ts$/.test(basename(file))) return builtinSource(file, ...rest);
  };
  host.resolveModuleNames = (specifiers, file) => specifiers.map(specifier => {
    const resolvedFileName = modules.get(file)?.dependencies.get(specifier);
    return resolvedFileName && (resolvedFileName === officialModulePath
      ? {resolvedFileName, extension: ts.Extension.Dts, isExternalLibraryImport: true}
      : {resolvedFileName, extension: ts.Extension.Ts});
  });
  const program = ts.createProgram([...modules.keys()], options, host);
  const checker = program.getTypeChecker();
  const errors = [...program.getSyntacticDiagnostics(), ...program.getSemanticDiagnostics()];
  if (errors.length) {
    const [error] = errors;
    const details = ts.flattenDiagnosticMessageText(error.messageText, '\n');
    const record = error.file && modules.get(error.file.fileName);
    if (!record || error.start === undefined) fail(details);
    const {line, character} = record.ast.getLineAndCharacterOfPosition(error.start);
    fail(`${record.name}:${line + 1}:${character + 1}: ${details}`);
  }
  for (const record of modules.values()) assertTypeScript(record.ast, checker, [], reason => fail(record.name + ': ' + reason));

  function helper(node) {
    if (!ts.isIdentifier(node)) return;
    let binding = checker.getSymbolAtLocation(node);
    if (binding?.flags & ts.SymbolFlags.Alias) binding = checker.getAliasedSymbol(binding);
    const declaration = binding?.valueDeclaration;
    const origin = declaration && modules.get(declaration.getSourceFile().fileName);
    if (origin && ts.isFunctionDeclaration(declaration) && declaration.parent === origin.ast && declaration.body) {
      return {declaration, origin};
    }
  }

  return {ast: modules.get(entry.declared).ast, helper, checker, hasOfficial};
}
