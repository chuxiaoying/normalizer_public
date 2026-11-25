import { realpathSync, statSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';

const outside = path => isAbsolute(path) || path === '..' || path.startsWith('..' + sep);

// Resolve authorized reads separately from the declared path used as provenance.
export function projectSources(root) {
  const scope = realpathSync(root);
  return file => {
    const declared = resolve(scope, file);
    const name = relative(scope, declared);
    if (!name || isAbsolute(file) || outside(name)) {
      throw new Error(`[SOURCE-OUTSIDE-ROOT] ${file} must be project-relative`);
    }
    const path = realpathSync(declared);
    if (outside(relative(scope, path))) {
      throw new Error(`[SOURCE-OUTSIDE-ROOT] ${name} resolves outside the selected source directory`);
    }
    if (!statSync(path).isFile()) throw new Error(`[SOURCE-READ-FAILED] ${name} is not a regular source file`);
    return { declared, name, path };
  };
}
