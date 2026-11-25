import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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

function invoke(command, args, cwd, input, env = process.env) {
  const result = spawnSync(command, args, { cwd, input, env, encoding: 'utf8', timeout: 120_000 });
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
  ['linked-directory-is-not-walked-recursively', project => {
    symlinkSync(project, join(project, 'Loop'));
    return project;
  }, /\[SOURCE-SYMLINK-DIRECTORY\].*Loop/],
  ['linked-file-must-not-leave-the-selected-root', project => {
    const root = join(project, 'selected');
    mkdirSync(root);
    const neighbor = join(project, 'Neighbor.java');
    writeFileSync(neighbor, 'class Probe { static int run() { return 7; } }');
    symlinkSync(neighbor, join(root, 'Probe.java'));
    return root;
  }, /\[SOURCE-OUTSIDE-ROOT\].*Probe\.java/],
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

const additionSource = count => '/**\n * @param {number} x\n * @returns {number}\n */\n'
  + 'export function sum(x) { return x' + ' + 1'.repeat(count) + '; }\n';

test('module CLI ingress / linked modules stay in the source root and keep their declared name', t => {
  const workspace = mkdtempSync(join(tmpdir(), 'normalizer-module-link-'));
  t.after(() => rmSync(workspace, {recursive: true, force: true}));
  const project = join(workspace, 'selected');
  mkdirSync(project);
  writeFileSync(join(project, 'local.txt'), additionSource(1));
  writeFileSync(join(workspace, 'outside.txt'), additionSource(1));
  symlinkSync(join(project, 'local.txt'), join(project, 'sum.js'));
  symlinkSync(join(workspace, 'outside.txt'), join(project, 'outside.js'));
  mkdirSync(join(project, 'directory.js'));
  const run = name => invoke(join(root, 'implementation/tools/normalizer'), ['module', project, name, 'sum'], root);
  const accepted = run('sum.js');
  assert.equal(accepted.status, 0, accepted.stderr);
  const actual = YAML.parse(accepted.stdout);
  assertPers(actual, 'in-root module link');
  assert.equal(actual.entry.function, 'sum.js#sum');
  assert.equal(actual.R, 'x + 1');
  for (const [name, diagnostic] of [
    ['outside.js', /\[SOURCE-OUTSIDE-ROOT\].*outside\.js/],
    ['../other.js', /\[SOURCE-OUTSIDE-ROOT\].*other\.js/],
    ['directory.js', /\[SOURCE-READ-FAILED\].*directory\.js/],
  ]) {
    const rejected = run(name);
    assert.notEqual(rejected.status, 0);
    assert.equal(rejected.stdout, '');
    assert.match(rejected.stderr, diagnostic);
  }
});

test('scenario CLI ingress / original and prepared Vue files stay in the same selected root', t => {
  const workspace = mkdtempSync(join(tmpdir(), 'normalizer-scenario-link-'));
  t.after(() => rmSync(workspace, {recursive: true, force: true}));
  const project = join(workspace, 'selected');
  mkdirSync(project);
  const original = '<template><Card :amount="7" /></template>\n';
  const prepared = '/** @componentProp Card#amount.value */\nexport function amount() { return 7; }\n';
  writeFileSync(join(project, 'local.vue'), original);
  writeFileSync(join(project, 'local.mjs'), prepared);
  writeFileSync(join(workspace, 'outside.vue'), original);
  writeFileSync(join(workspace, 'outside.mjs'), prepared);
  symlinkSync(join(project, 'local.vue'), join(project, 'Page.vue'));
  symlinkSync(join(project, 'local.mjs'), join(project, 'page.mjs'));
  symlinkSync(join(workspace, 'outside.vue'), join(project, 'outside.vue'));
  symlinkSync(join(workspace, 'outside.mjs'), join(project, 'outside.mjs'));
  const run = (module, component) => invoke(join(root, 'implementation/tools/normalizer'),
    ['scenario', 'frontend-vo-consumption', project, '--entry', `${module}:amount`, '--source', component], root);
  const accepted = run('page.mjs', 'Page.vue');
  assert.equal(accepted.status, 0, accepted.stderr);
  const actual = YAML.parse(accepted.stdout);
  assertPers(actual, 'in-root Vue links');
  assert.equal(actual.entry.function, 'Page.vue#Card#amount.value');
  assert.equal(actual.R, '7');
  for (const [module, component] of [['page.mjs', 'outside.vue'], ['outside.mjs', 'Page.vue']]) {
    const rejected = run(module, component);
    assert.notEqual(rejected.status, 0);
    assert.equal(rejected.stdout, '');
    assert.match(rejected.stderr, /\[SOURCE-OUTSIDE-ROOT\].*outside\.(vue|mjs)/);
  }
});

test('module CLI ingress / expression nesting is bounded before execution', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-module-depth-'));
  t.after(() => rmSync(project, {recursive: true, force: true}));
  const module = join(root, 'implementation/tools/normalizer');
  for (const count of [120, 200]) {
    writeFileSync(join(project, 'sum.js'), additionSource(count));
    const result = invoke(module, ['module', project, 'sum.js', 'sum'], root);
    if (count === 120) {
      assert.equal(result.status, 0, result.stderr);
      const actual = YAML.parse(result.stdout);
      assertPers(actual, 'large but valid exact addition');
      assert.equal(actual.R, 'x + 120');
    } else {
      assert.notEqual(result.status, 0);
      assert.equal(result.stdout, '');
      assert.match(result.stderr, /recursion limit exceeded/);
      assert.doesNotMatch(result.stderr, /panicked/);
    }
  }
});


test('module CLI execution / exhausted normalization leaves no misleading PERS', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-module-budget-'));
  t.after(() => rmSync(project, {recursive: true, force: true}));
  writeFileSync(join(project, 'sum.js'), additionSource(64));
  const command = join(root, 'implementation/tools/normalizer');
  const args = ['module', project, 'sum.js', 'sum'];
  const ordinary = invoke(command, args, root);
  assert.equal(ordinary.status, 0, ordinary.stderr);
  const full = YAML.parse(ordinary.stdout);
  assertPers(full, 'fully normalized exact addition');
  assert.equal(full.R, 'x + 64');
  const constrained = invoke(command, args, root, undefined, {...process.env, NORMALIZER_MATH_MAX_STEPS: '1'});
  assert.notEqual(constrained.status, 0);
  assert.equal(constrained.stdout, '');
  assert.match(constrained.stderr, /normalization steps budget exhausted/);
  assert.match(constrained.stderr, /no canonical result was produced/);
});

test('source CLI execution / bounded cache eviction does not change normalization', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-source-cache-'));
  t.after(() => rmSync(project, {recursive: true, force: true}));
  const count = 55_000;
  writeFileSync(join(project, 'CacheProbe.java'),
    'class CacheProbe { static int sum(int x) { int y = x;\n'
    + 'y = y + 1;\n'.repeat(count) + 'return y; } }\n');
  const profile = join(project, 'math-profile.log');
  const env = Object.fromEntries(Object.entries(process.env)
    .filter(([name]) => !name.startsWith('NORMALIZER_MATH_')));
  env.NORMALIZER_MATH_PROFILE_LOG = profile;
  const result = invoke(join(root, 'implementation/tools/normalizer'),
    ['source', project, 'CacheProbe.sum'], root, undefined, env);
  assert.equal(result.status, 0, result.stderr);
  const actual = YAML.parse(result.stdout);
  assertPers(actual, 'bounded cache source result');
  assert.equal(actual.R, 'x + ' + count);
  assert.match(readFileSync(profile, 'utf8'),
    /math\.cache\.evict category=simplify_once before=100000 removed=50000 after=50000/);
});


test('source CLI ingress / in-root source links keep the declared source path', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-source-link-'));
  t.after(() => rmSync(project, {recursive: true, force: true}));
  const content = join(project, 'content.txt');
  writeFileSync(content, 'class Probe { static int run() { return 7; } }');
  symlinkSync(content, join(project, 'Probe.java'));
  const result = invoke(join(root, 'implementation/tools/normalizer'),
    ['source', project, 'Probe.run'], root);
  assert.equal(result.status, 0, result.stderr);
  const actual = YAML.parse(result.stdout);
  assertPers(actual, 'in-root source link');
  assert.equal(actual.entry.function, 'Probe.java#Probe.run');
  assert.equal(actual.R, '7');
});
