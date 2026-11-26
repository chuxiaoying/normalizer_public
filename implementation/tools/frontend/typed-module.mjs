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

export function typedBooleanParameters(fn) {
  const bound = new Set();
  return fn.parameters.map(param => {
    bound.add(param.name.text);
    return param.name.text;
  });
}

export function checkedTypeScript(physicalPath, sf, allowed) {
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
