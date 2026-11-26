import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyVariant, explicitVariantEntry } from './business_example_series.mjs';
import { assertPers } from './pers-schema.mjs';
import { assertRejection } from './rejection.mjs';
import { writePreparedSource } from './prepared-source.mjs';
import { loadSeedEntries } from './seed-example.mjs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const root = fileURLToPath(new URL('../../', import.meta.url));

function invoke(command, args, cwd, input, env = process.env) {
  const result = spawnSync(command, args, { cwd, input, env, encoding: 'utf8', timeout: 120_000 });
  if (result.error) throw result.error;
  assert.equal(result.signal, null, command + ' terminated by signal');
  return result;
}

function checkExample(project, scene, entry, fixture, variant, seedReject) {
  const { patch, source, sourceFile, mainFile, entryFunction: seedEntry } = fixture;
  const override = explicitVariantEntry(variant);
  if (override) assert.ok(mainFile.endsWith('.java'), 'Entry override requires a Java seed');
  const entryFunction = override ?? seedEntry;
  writePreparedSource(project, sourceFile, source, patch);
  const expected = variant ? applyVariant(project, mainFile, fixture.expected, variant) : fixture.expected;
  if (override && !variant.reject && !seedReject) {
    assert.equal(expected.entry.function, override, 'A changed fixture entry must be independently asserted');
  }
  let args;
  if (fixture.sourceLanguage === 'vue') {
    assert.ok(scene, 'Vue source requires a registered CLI scene');
    args = ['scenario', scene, project, '--entry', entryFunction, '--source', sourceFile];
  } else if (mainFile.endsWith('.ts')) {
    args = ['module', project, '--entry', entryFunction];
  } else {
    assert.equal(scene, undefined, 'Java source must use the source CLI route');
    args = ['source', project, entryFunction];
  }
  const { status, stdout, stderr } = invoke(join(root, 'implementation/tools/normalizer'), args, root);
  const fragments = variant?.reject ?? seedReject;
  if (fragments) {
    const kind = !variant?.reject ? fixture.rejection.split('\n')[0] : undefined;
    assertRejection({ status, stdout, stderr }, !variant?.reject && seedReject.includes('[SOURCE-PARSE-FAILED]') ? fixture.sourceFile : entryFunction, fragments, kind);
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
