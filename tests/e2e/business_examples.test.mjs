import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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


// These are CLI ingress contracts, not Java business seeds. The failure must
// be diagnosed before normalizing or publishing any PERS.
for (const [name, setup, diagnostic] of [
  ['missing-root', project => join(project, 'does-not-exist'), '[NORM-SOURCE] expected existing directory'],
  ['empty-source-root', project => project, '[SOURCE-EMPTY]'],
  ['missing-entry-owner-is-not-inferred-from-another-class', project => {
    writeFileSync(join(project, 'Other.java'), 'class Other { static int run() { return 7; } }');
    return project;
  }, /\[NORM-ENTRY\].*Probe\.run/],
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
      join(root, 'implementation/tools/normalizer'), ['source', setup(project), 'Probe.java#Probe.run'], root);
    assert.notEqual(status, 0);
    assert.equal(stdout, '');
    assert.ok(typeof diagnostic === 'string' ? stderr.includes(diagnostic) : diagnostic.test(stderr), stderr);
    assert.ok(!stderr.includes('panicked'), stderr);
  });
}

const additionSource = count =>
  'export function sum(x: number): number { return x' + ' + 1'.repeat(count) + '; }\n';

test('module CLI ingress / linked modules stay in the source root and keep their declared name', t => {
  const workspace = mkdtempSync(join(tmpdir(), 'normalizer-module-link-'));
  t.after(() => rmSync(workspace, {recursive: true, force: true}));
  const project = join(workspace, 'selected');
  mkdirSync(project);
  writeFileSync(join(project, 'local.ts'), additionSource(1));
  writeFileSync(join(workspace, 'outside-source.ts'), additionSource(1));
  symlinkSync(join(project, 'local.ts'), join(project, 'sum.ts'));
  symlinkSync(join(workspace, 'outside-source.ts'), join(project, 'outside.ts'));
  mkdirSync(join(project, 'directory.ts'));
  const run = name => invoke(join(root, 'implementation/tools/normalizer'), ['module', project, '--entry', name + '#sum'], root);
  const accepted = run('sum.ts');
  assert.equal(accepted.status, 0, accepted.stderr);
  const actual = YAML.parse(accepted.stdout);
  assertPers(actual, 'in-root module link');
  assert.equal(actual.entry.function, 'sum.ts#sum');
  assert.equal(actual.R, 'x + 1');
  for (const [name, diagnostic] of [
    ['outside.ts', /\[SOURCE-OUTSIDE-ROOT\].*outside\.ts/],
    ['../other.ts', /\[SOURCE-OUTSIDE-ROOT\].*other\.ts/],
    ['directory.ts', /\[SOURCE-READ-FAILED\].*directory\.ts/],
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
  const prepared = '/** @N componentProp Card#amount.value */\nexport function amount(): number { return 7; }\n';
  writeFileSync(join(project, 'local.vue'), original);
  writeFileSync(join(project, 'local.ts'), prepared);
  writeFileSync(join(workspace, 'outside.vue'), original);
  writeFileSync(join(workspace, 'outside.ts'), prepared);
  symlinkSync(join(project, 'local.vue'), join(project, 'Page.vue'));
  symlinkSync(join(project, 'local.ts'), join(project, 'page.ts'));
  symlinkSync(join(workspace, 'outside.vue'), join(project, 'outside.vue'));
  symlinkSync(join(workspace, 'outside.ts'), join(project, 'outside.ts'));
  const run = (module, component) => invoke(join(root, 'implementation/tools/normalizer'),
    ['scenario', 'frontend-vo-consumption', project, '--entry', `${module}#amount`, '--source', component], root);
  const accepted = run('page.ts', 'Page.vue');
  assert.equal(accepted.status, 0, accepted.stderr);
  const actual = YAML.parse(accepted.stdout);
  assertPers(actual, 'in-root Vue links');
  assert.equal(actual.entry.function, 'page.ts#amount');
  assert.equal(actual.R, '7');
  for (const [module, component] of [['page.ts', 'outside.vue'], ['outside.ts', 'Page.vue']]) {
    const rejected = run(module, component);
    assert.notEqual(rejected.status, 0);
    assert.equal(rejected.stdout, '');
    assert.match(rejected.stderr, /\[SOURCE-OUTSIDE-ROOT\].*outside\.(vue|ts)/);
  }
});

test('module CLI ingress / expression nesting is bounded before execution', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-module-depth-'));
  t.after(() => rmSync(project, {recursive: true, force: true}));
  const module = join(root, 'implementation/tools/normalizer');
  for (const count of [120, 200]) {
    writeFileSync(join(project, 'sum.ts'), additionSource(count));
    const result = invoke(module, ['module', project, '--entry', 'sum.ts#sum'], root);
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
  writeFileSync(join(project, 'sum.ts'), additionSource(64));
  const command = join(root, 'implementation/tools/normalizer');
  const args = ['module', project, '--entry', 'sum.ts#sum'];
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

test('source CLI execution / proof and cache admission limits preserve the nested CASE', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-source-proof-budget-'));
  t.after(() => rmSync(project, { recursive: true, force: true }));
  writePreparedSource(project, 'src/ProofBudget.java', `
class ProofBudget {
    static int nested(int x, int y, boolean p) {
        if (x < 0) return p ? 10 : 20;
        if (y > 1) return 20;
        return 30;
    }
}
`);
  const expected = {
    schema: 'normalizer.pers/1',
    entry: { function: 'src/ProofBudget.java#ProofBudget.nested', returns: 'number' },
    P: { parameters: {
      x: { position: 0, type: 'number' },
      y: { position: 1, type: 'number' },
      p: { position: 2, type: 'bool' },
    } },
    // x < 0 is first: p yields 10, otherwise 20; only the other region
    // can choose 20 via y > 1 or fall through to 30.
    R: 'CASE\n  WHEN p AND x < 0 THEN 10\n  WHEN x < 0 OR y > 1 THEN 20\n  ELSE 30\nEND',
  };
  const env = Object.fromEntries(Object.entries(process.env)
    .filter(([name]) => !name.startsWith('NORMALIZER_MATH_')));
  const command = join(root, 'implementation/tools/normalizer');
  const args = ['source', project, 'src/ProofBudget.java#ProofBudget.nested'];
  const profile = join(project, 'proof-budget.log');
  for (const options of [
    {},
    { NORMALIZER_MATH_MAX_PROOF_NODES: '1', NORMALIZER_MATH_PROFILE_LOG: profile },
    { NORMALIZER_MATH_MAX_CACHE_ENTRY_NODES: '1' },
  ]) {
    const result = invoke(command, args, root, undefined, { ...env, ...options });
    assert.equal(result.status, 0, result.stderr);
    const actual = YAML.parse(result.stdout);
    assertPers(actual, 'nested CASE with constrained math resources');
    assert.deepEqual(actual, expected);
  }
  assert.match(readFileSync(profile, 'utf8'),
    /math\.proof\.path_implies\.budget_skip .*reason=run_proof_budget/);
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
    ['source', project, 'CacheProbe.java#CacheProbe.sum'], root, undefined, env);
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
    ['source', project, 'Probe.java#Probe.run'], root);
  assert.equal(result.status, 0, result.stderr);
  const actual = YAML.parse(result.stdout);
  assertPers(actual, 'in-root source link');
  assert.equal(actual.entry.function, 'Probe.java#Probe.run');
  assert.equal(actual.R, '7');
});

test('complemented CASE has one representative in pure returns and state writes', t => {
  const project = mkdtempSync(join(tmpdir(), 'normalizer-case-orientation-'));
  t.after(() => rmSync(project, { recursive: true, force: true }));
  writePreparedSource(project, 'src/Compare.java', `
import normalizer.intrinsics.N;
class Compare {
    static final Compare INSTANCE = new Compare();
    @N.EntryState int result;
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
    const entry = 'src/Compare.java#Compare.' + (name.startsWith('state') ? 'INSTANCE.' : '') + name;
    const { status, stdout, stderr } = invoke(join(root, 'implementation/tools/normalizer'), ['source', project, entry], root);
    assert.equal(status, 0, stderr);
    const actual = YAML.parse(stdout);
    assertPers(actual, entry);
    const expected = {
      schema: 'normalizer.pers/1',
      entry: { function: entry, returns: 'number' },
      P: { parameters: { a: { position: 0, type: 'number' }, b: { position: 1, type: 'number' } } },
      R: result,
    };
    if (name.startsWith('state')) {
      expected.E = { result: { kind: 'state', source: 'src/Compare.java#Compare.INSTANCE.result', type: 'number' } };
      expected.S = [{ write: { target: 'result', value: result } }];
    }
    assert.deepEqual(actual, expected);
  }
});
