import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, posix } from 'node:path';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { loadCases } from './catalog.mjs';
import { diagnosticCode } from './rejection.mjs';

const sourcePath = /^src\/(?:[\w$.-]+\/)*[\w$.-]+\.(?:java|ts)$/;
const caseName = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const sourceDirectory = /^[\w$.-]+$/;

// Walk the project as checked in, keeping relative paths independent of the host OS.
function readSources(root, directory, relative = 'src') {
  return readdirSync(join(root, directory, relative), { withFileTypes: true }).flatMap(item => {
    const path = posix.join(relative, item.name);
    if (item.isDirectory()) {
      assert.match(item.name, sourceDirectory, directory + ': invalid source directory ' + path);
      return readSources(root, directory, path);
    }
    assert.ok(item.isFile() && sourcePath.test(path), directory + ': unexpected source file ' + path);
    return [[path, readFileSync(join(root, directory, path), 'utf8')]];
  });
}

function loadFixture(root, directory, language, entry, scene, inputs) {
  const file = directory + '/case.yaml';
  const children = readdirSync(join(root, directory), { withFileTypes: true });
  assert.ok(children.length === 2 && children.some(item => item.name === 'case.yaml' && item.isFile()) &&
    children.some(item => item.name === 'src' && item.isDirectory()),
    directory + ': expected exactly case.yaml and a src directory');
  const fixture = YAML.parse(readFileSync(join(root, file), 'utf8'));
  const { entry: locator, expected, reject } = fixture;
  assert.notEqual(Object.hasOwn(fixture, 'expected'), Object.hasOwn(fixture, 'reject'),
    file + ': declare exactly one outcome');
  assert.deepEqual(Object.keys(fixture).sort(),
    ['entry', reject ? 'reject' : 'expected'], file);
  const [main, symbol, extra] = typeof locator === 'string' ? locator.split('#') : [];
  assert.ok(symbol && extra === undefined && sourcePath.test(main), file + ': invalid CLI entry locator');
  const sources = readSources(root, directory).sort(([left], [right]) => left.localeCompare(right));
  const input = Object.fromEntries(sources);
  assert.ok(Object.hasOwn(input, main), file + ': the entry source must be in the fixture');
  if (scene) {
    assert.ok(main.endsWith('.ts') && symbol === entry, file + ': invalid prepared frontend locator');
  } else if (language === 'typescript') {
    assert.equal(locator, 'src/' + entry, file + ': standalone TypeScript locator must match the catalog');
  } else {
    assert.ok(main.endsWith('.java'), file + ': Java case requires a Java input');
  }
  if (reject) {
    assert.ok(Object.keys(reject).every(key => ['fragments', 'kind', 'location'].includes(key)), file);
    assert.ok(Array.isArray(reject.fragments) && reject.fragments.every(text => typeof text === 'string' && text.trim()) &&
      diagnosticCode(reject.fragments[0]), file + ': rejection must name a primary diagnostic');
    if (reject.kind !== undefined) assert.match(reject.kind, /^[A-Z_]+$/, file);
    if (reject.location !== undefined) assert.match(reject.location, sourcePath, file);
  } else {
    assert.equal(expected?.entry?.function, locator, file + ': the complete PERS must name this entry');
  }

  const key = JSON.stringify([language, scene ?? null, locator, sources]);
  assert.ok(!inputs.has(key), 'Duplicate CLI input: ' + inputs.get(key) + ' and ' + file);
  inputs.set(key, file);
  return { file, directory, input, ...fixture };
}

export function* loadFixtureEntries(root, seeds) {
  const inputs = new Map();
  for (const registration of loadCases(root, seeds)) {
    const { entry, group, scene, caseDirectory } = registration;
    const language = group.split('/')[0];
    const files = readdirSync(join(root, caseDirectory), { withFileTypes: true });
    assert.ok(files.some(file => file.name === 'original'), caseDirectory + ': missing original');
    assert.ok(files.every(file => file.isDirectory() && caseName.test(file.name)), caseDirectory + ': unexpected fixture item');
    const names = ['original', ...files.map(file => file.name).filter(name => name !== 'original').sort()];
    const cases = names.map(name => ({
      name, ...loadFixture(root, caseDirectory + '/' + name, language, entry, scene, inputs),
    }));
    yield { ...registration, language, entry: language === 'typescript' ? entry.split('#')[1] : entry, cases };
  }
}
