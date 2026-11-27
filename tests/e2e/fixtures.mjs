import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { loadCases } from './catalog.mjs';
import { diagnosticCode } from './rejection.mjs';

const sourcePath = /^src\/[\w-]+\.(?:java|vue|mjs|js|ts)$/;
const caseName = /^[a-z0-9]+(?:-[a-z0-9]+)*\.yaml$/;
const entryPath = /^(src\/[\w-]+\.(?:java|ts))#([^#]+)$/;

function loadFixture(root, file, language, entry, scene, inputs) {
  const fixture = YAML.parse(readFileSync(join(root, file), 'utf8'));
  const { entry: locator, vueSource, input, expected, reject } = fixture;
  assert.notEqual(Object.hasOwn(fixture, 'expected'), Object.hasOwn(fixture, 'reject'),
    file + ': declare exactly one outcome');
  assert.deepEqual(Object.keys(fixture).sort(),
    ['entry', 'input', scene && 'vueSource', reject ? 'reject' : 'expected'].filter(Boolean).sort(), file);
  const [, main, symbol] = entryPath.exec(locator) ?? [];
  assert.ok(main && symbol, file + ': invalid CLI entry locator');
  assert.ok(input && typeof input === 'object' && !Array.isArray(input), file + ': missing prepared project');
  const sources = Object.entries(input).sort(([left], [right]) => left.localeCompare(right));
  assert.ok(sources.every(([path, text]) => sourcePath.test(path) && typeof text === 'string'),
    file + ': each input file must have a legal source path and complete text');
  assert.ok(Object.hasOwn(input, main), file + ': the entry source must be in the fixture');
  if (scene) {
    assert.ok(main.endsWith('.ts') && symbol === entry && sourcePath.test(vueSource) &&
      vueSource.endsWith('.vue') && Object.hasOwn(input, vueSource), file + ': invalid Vue source or locator');
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

  // Vue's original file is copied only for attribution, not read for analysis.
  const key = JSON.stringify([language, scene ?? null, locator, vueSource ?? null,
    sources.filter(([path]) => path !== vueSource)]);
  assert.ok(!inputs.has(key), 'Duplicate CLI input: ' + inputs.get(key) + ' and ' + file);
  inputs.set(key, file);
  return fixture;
}

export function* loadFixtureEntries(root, seeds) {
  const inputs = new Map();
  for (const registration of loadCases(root, seeds)) {
    const { entry, group, scene, caseDirectory } = registration;
    const language = group.split('/')[0];
    const files = readdirSync(join(root, caseDirectory), { withFileTypes: true });
    assert.ok(files.some(file => file.name === 'original.yaml'), caseDirectory + ': missing original');
    assert.ok(files.every(file => file.isFile() && caseName.test(file.name)), caseDirectory + ': unexpected fixture file');
    const names = ['original.yaml', ...files.map(file => file.name).filter(name => name !== 'original.yaml').sort()];
    const cases = names.map(name => {
      const file = caseDirectory + '/' + name;
      return { name: name.slice(0, -5), file, ...loadFixture(root, file, language, entry, scene, inputs) };
    });
    yield { ...registration, language, entry: language === 'typescript' ? entry.split('#')[1] : entry, cases };
  }
}
