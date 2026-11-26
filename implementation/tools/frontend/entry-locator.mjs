// A locator names a prepared source path and an exported binding, not a call with values.
export function entryLocator(locator, fail) {
  const [file, name, ...rest] = typeof locator === 'string' ? locator.split('#') : [];
  // Scoped source reading remains the responsibility of projectSources. The
  // lexical form here deliberately excludes absolute paths and dot components.
  if (file?.startsWith('/') || file?.split('/').includes('..')) {
    fail('[SOURCE-OUTSIDE-ROOT] ' + file + ' must be a canonical project-relative path');
  }
  if (rest.length || !file || !name || file.split('/').some(part => !part || part === '.')
      || /[\\:\0]/.test(file) || !/^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*$/.test(name)) {
    fail('expected a project-relative source path#exported entry: ' + locator);
  }
  return {file, name};
}
