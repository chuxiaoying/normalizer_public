// The typed standalone syntactic and type-checking gate shared by .ts shapes.
import ts from 'typescript';
import {localDirectives} from './local-directives.mjs';

export const failTypedModule = message => { throw new Error('[NORM-TS-MODULE] ' + message); };
export const typedIdentifier = x => typeof x === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(x) && x !== 'undefined';

export function typedModuleAst(source, physicalPath, entry) {
  const sf = ts.createSourceFile(physicalPath, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  if (sf.parseDiagnostics.length) failTypedModule(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  const exports = sf.statements.filter(dec => ts.isFunctionDeclaration(dec)
    && dec.name?.text === entry && dec.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword));
  if (exports.length !== 1 || !exports[0].body) failTypedModule('expected one defined exported function: ' + entry);
  for (const declaration of sf.statements) {
    if (ts.isFunctionDeclaration(declaration) && declaration.body) localDirectives(sf, declaration.body).requireConsumed();
  }
  return {sf, fn: exports[0]};
}

export function typedBooleanParameters(fn) {
  const bound = new Set();
  return fn.parameters.map(param => {
    if (!ts.isIdentifier(param.name) || !typedIdentifier(param.name.text) || bound.has(param.name.text)
        || param.initializer || param.dotDotDotToken || param.questionToken
        || param.type?.kind !== ts.SyntaxKind.BooleanKeyword) failTypedModule('typed module requires distinct Boolean parameters');
    bound.add(param.name.text);
    return param.name.text;
  });
}

export function checkedTypeScript(physicalPath, sf, allowed) {
  if (sf.referencedFiles.length || sf.typeReferenceDirectives.length || sf.libReferenceDirectives.length
      || sf.statements.some(node => !allowed(node))) {
    failTypedModule('typed module requires closed declarations');
  }
  let hasImport = false;
  const inspect = node => {
    if (ts.isImportTypeNode(node) || node.kind === ts.SyntaxKind.ImportKeyword) hasImport = true;
    ts.forEachChild(node, inspect);
  };
  inspect(sf);
  if (hasImport) failTypedModule('typed module cannot import outside declarations');
  const options = {strict: true, noEmit: true, noResolve: true, types: [], target: ts.ScriptTarget.ES2022};
  const host = ts.createCompilerHost(options);
  const readSource = host.getSourceFile.bind(host);
  host.getSourceFile = (name, ...args) => name === physicalPath ? sf : readSource(name, ...args);
  const checked = ts.createProgram([physicalPath], options, host);
  const errors = [...checked.getSyntacticDiagnostics(), ...checked.getSemanticDiagnostics()];
  if (errors.length) failTypedModule(ts.flattenDiagnosticMessageText(errors[0].messageText, '\n'));
  return checked.getTypeChecker();
}
