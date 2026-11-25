import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyVariant } from './business_example_series.mjs';
import { assertPers } from './pers-schema.mjs';
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
  } else if (['javascript', 'typescript'].includes(fixture.sourceLanguage)) {
    args = ['module', project, sourceFile, entry];
  } else {
    assert.equal(scene, undefined, 'Java source must use the source CLI route');
    args = ['source', project, entry];
  }
  const { status, stdout, stderr } = invoke(join(root, 'implementation/tools/normalizer'), args, root);
  const fragments = variant?.reject ?? seedReject;
  if (fragments) {
    assert.notEqual(status, 0, 'Expected a diagnostic');
    assert.equal(stdout, '');
    if (!variant?.reject) {
      // A declaration parser error precedes entry selection and instead
      // must point to the supplied source file and the offending token.
      const parseFailure = seedReject.includes('[SOURCE-PARSE-FAILED]');
      assert.ok(stderr.includes(parseFailure ? fixture.sourceFile : entry), stderr);
      const kind = fixture.rejection.split('\n')[0];
      assert.ok(stderr.split(/\s+/).includes('kind=' + kind), stderr);
    }
    for (const fragment of fragments) assert.ok(stderr.includes(fragment), stderr);
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


// These are CLI ingress contracts, not Java business seeds. The failure must
// be diagnosed before normalizing or publishing any PERS.
for (const [name, setup, diagnostic] of [
  ['missing-root', project => join(project, 'does-not-exist'), '[NORM-SOURCE] expected existing directory'],
  ['empty-source-root', project => project, '[SOURCE-EMPTY]'],
  ['malformed-source-utf8', project => {
    writeFileSync(join(project, 'Bad.java'), Buffer.from([0xff]));
    return project;
  }, '[SOURCE-DECODE-FAILED]'],
  ['read-failure-keeps-source-name', project => {
    symlinkSync(join(project, 'missing.java'), join(project, 'Dangling.java'));
    return project;
  }, /\[SOURCE-READ-FAILED\].*Dangling\.java/],
  ['non-utf8-source-name-is-a-diagnostic-not-a-panic', project => {
    writeFileSync(Buffer.concat([Buffer.from(project + '/'), Buffer.from([0xff]), Buffer.from('.java')]), '');
    return project;
  }, '[SOURCE-PATH-FAILED]'],
]) {
  test('source CLI ingress / ' + name, t => {
    const project = mkdtempSync(join(tmpdir(), 'normalizer-ingress-'));
    t.after(() => rmSync(project, { recursive: true, force: true }));
    const { status, stdout, stderr } = invoke(
      join(root, 'implementation/tools/normalizer'), ['source', setup(project), 'Probe.run'], root);
    assert.notEqual(status, 0);
    assert.equal(stdout, '');
    assert.ok(typeof diagnostic === 'string' ? stderr.includes(diagnostic) : diagnostic.test(stderr), stderr);
    assert.ok(!stderr.includes('panicked'), stderr);
  });
}
