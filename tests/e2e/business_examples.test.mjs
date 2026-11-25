import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyVariant } from './business_example_series.mjs';
import { assertPers } from './pers-schema.mjs';
import { assertRejection } from './rejection.mjs';
import { writePreparedSource } from './prepared-source.mjs';
import { loadSeedEntries } from './seed-example.mjs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const root = fileURLToPath(new URL('../../', import.meta.url));

function invoke(command, args, cwd, input) {
  const result = spawnSync(command, args, { cwd, input, encoding: 'utf8', timeout: 120_000 });
  if (result.error) throw result.error;
  assert.equal(result.signal, null, command + ' terminated by signal');
  return result;
}

function checkExample(project, scene, entry, fixture, variant, seedReject) {
  const { patch, source, sourceFile, mainFile } = fixture;
  writePreparedSource(project, sourceFile, source, patch);
  const expected = variant ? applyVariant(project, mainFile, fixture.expected, variant) : fixture.expected;
  let args;
  if (fixture.sourceLanguage === 'vue') {
    assert.ok(scene, 'Vue source requires a registered CLI scene');
    const exported = entry.split('.').at(-1);
    args = ['scenario', scene, project, '--entry', mainFile + ':' + exported, '--source', sourceFile];
  } else if (fixture.sourceLanguage === 'javascript') {
    args = ['module', project, sourceFile, entry];
  } else {
    assert.equal(scene, undefined, 'Java source must use the source CLI route');
    args = ['source', project, entry];
  }
  const { status, stdout, stderr } = invoke(join(root, 'implementation/tools/normalizer'), args, root);
  const fragments = variant?.reject ?? seedReject;
  if (fragments) {
    const kind = !variant?.reject ? fixture.rejection.split('\n')[0] : undefined;
    assertRejection({ status, stdout, stderr }, entry, fragments, kind);
    return;
  }
  assert.equal(status, 0, stderr.trim() || 'Normalizer failed without a diagnostic');
  const actual = YAML.parse(stdout);
  assertPers(expected, entry + ' expected');
  assertPers(actual, entry + ' actual');
  assert.deepEqual(actual, expected);
}

for (const { seedId, entry, scene, fixture, variants, reject } of loadSeedEntries(root)) {
  for (const variant of [null, ...variants]) {
    test([seedId, entry, variant?.name ?? 'original'].join(' / '), t => {
      const project = mkdtempSync(join(tmpdir(), 'normalizer-example-'));
      t.after(() => rmSync(project, { recursive: true, force: true }));
      checkExample(project, scene, entry, fixture, variant, reject);
    });
  }
}

test('complemented CASE has one representative in pure returns and state writes', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-case-orientation-'));
  t.after(() => rmSync(project, { recursive: true, force: true }));
  writePreparedSource(project, 'src/Compare.java', `
import normalizer.annotations.EntryState;
class Compare {
    @EntryState int result;
    static int pure(int a, int b) { return a == 1 && b == 2 ? 7 : 9; }
    static int inverse(int a, int b) { if (a != 1 || b != 2) return 9; return 7; }
    int state(int a, int b) { result = a == 1 && b == 2 ? 7 : 9; return result; }
    int stateInverse(int a, int b) { result = a != 1 || b != 2 ? 9 : 7; return result; }
}
`);
  // Only (1, 2) selects 7; all other inputs select 9. Both operations
  // use the exact same total condition, and neither reads initial state.
  const result = 'CASE\n  WHEN a == 1 AND b == 2 THEN 7\n  ELSE 9\nEND';
  for (const name of ['pure', 'inverse', 'state', 'stateInverse']) {
    const entry = 'Compare.' + name;
    const { status, stdout, stderr } = invoke(join(root, 'implementation/tools/normalizer'), ['source', project, entry], root);
    assert.equal(status, 0, stderr);
    const actual = YAML.parse(stdout);
    assertPers(actual, entry);
    const expected = {
      schema: 'normalizer.pers/1',
      entry: { function: 'src/Compare.java#' + entry, returns: 'number' },
      P: { parameters: { a: { position: 0, type: 'number' }, b: { position: 1, type: 'number' } } },
      R: result,
    };
    if (name.startsWith('state')) {
      expected.E = { result: { kind: 'state', source: 'src/Compare.java#Compare.result', type: 'number' } };
      expected.S = [{ write: { target: 'result', value: result } }];
    }
    assert.deepEqual(actual, expected);
  }
});
