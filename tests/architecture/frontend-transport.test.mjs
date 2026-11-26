import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const cli = join(root, 'implementation/tools/normalizer');
const scalarModule = 'export function value(): number { return 7; }\n';

function fixture(t, source, worker, file = 'value.ts', targetFile = file) {
  const directory = mkdtempSync(join(tmpdir(), 'normalizer-frontend-transport-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const target = join(directory, 'target');
  const project = join(directory, 'project');
  mkdirSync(join(target, 'debug'), { recursive: true });
  mkdirSync(project);
  writeFileSync(join(target, 'debug/normalizer-normalized'), worker, { mode: 0o700 });
  writeFileSync(join(project, targetFile), source);
  if (targetFile !== file) symlinkSync(targetFile, join(project, file));
  return (args = ['module', project, '--entry', file + '#value']) => {
    const result = spawnSync(cli, args.map(arg => arg === '<project>' ? project : arg), {
      cwd: root, encoding: 'utf8', timeout: 30_000,
      env: { ...process.env, CARGO_TARGET_DIR: target, CARGO: '/bin/false' },
    });
    assert.ifError(result.error);
    assert.equal(result.signal, null);
    return result;
  };
}

test('a missing worker interpreter is reported as launch failure with no PERS', t => {
  // Executable file exists, but POSIX spawn cannot resolve its interpreter.
  const run = fixture(t, scalarModule, '#!/.normalizer-fixture-no-such-interpreter\n');
  const result = run();
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /spawnSync .*normalizer-normalized ENOENT/);
});

test('a syntax-recovery tree never reaches the normalization worker', t => {
  const run = fixture(t, 'export function value( {', '#!/bin/sh\nprintf "unexpected worker invocation\\n" >&2\nexit 47\n');
  const result = run();
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /\[NORM-TS-MODULE\]/);
  assert.doesNotMatch(result.stderr, /unexpected worker invocation/);
});

const forbiddenWorker = '#!/bin/sh\nprintf "unexpected worker invocation\\n" >&2\nexit 47\n';
for (const file of ['value.js', 'value.mjs', 'value.d.ts']) {
  test(`the standalone CLI rejects ${file} before invoking the worker`, t => {
    const result = fixture(t, 'export function value() { return 7; }', forbiddenWorker, file)();
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /must be an executable \.ts source file/);
    assert.doesNotMatch(result.stderr, /unexpected worker invocation/);
  });
}

test('a TypeScript locator cannot disguise a JavaScript symlink target', t => {
  const result = fixture(t, scalarModule, forbiddenWorker, 'value.ts', 'value.js')();
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /must resolve to an executable \.ts source file/);
  assert.doesNotMatch(result.stderr, /unexpected worker invocation/);
});

for (const [description, extra, diagnostic] of [
  ['explicit any in an unused helper', 'function ignored(value: any) { return value; }', /explicit any/],
  ['an implicitly untyped parameter', 'function ignored(value) { return value; }', /implicitly has an .any. type/],
  ['any returned by the standard library', "function ignored() { return JSON.parse('{}'); }", /resolved any/],
  ['any inside an inferred array', 'function ignored() { return Array(1); }', /resolved any/],
  ['any named indirectly by a type expression', 'type Dynamic = ReturnType<typeof JSON.parse>;', /resolved any/],
  ['a file-wide disabled type check', '// @ts-nocheck\nfunction ignored(value) { return value; }', /cannot disable or suppress/],
  ['a suppressed type error', 'function ignored() {\n// @ts-ignore\nconst n: number = false;\n}', /cannot disable or suppress/],
  ['an expected type error used as a suppression', 'function ignored() {\n// @ts-expect-error\nconst n: number = false;\n}', /cannot disable or suppress/],
]) {
  test(`standalone type preflight rejects ${description} before invoking the worker`, t => {
    const result = fixture(t, extra + '\n' + scalarModule, forbiddenWorker)();
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /\[NORM-TS-MODULE\]/);
    assert.match(result.stderr, diagnostic);
    assert.doesNotMatch(result.stderr, /unexpected worker invocation/);
  });
}

test('worker failure keeps its status and diagnostic but discards incomplete output', t => {
  const run = fixture(t, scalarModule, '#!/bin/sh\nprintf "incomplete result\\n"\nprintf "worker diagnostic\\n" >&2\nexit 47\n');
  const result = run();
  assert.equal(result.status, 47, result.stderr);
  assert.equal(result.stderr, 'worker diagnostic\n');
  assert.equal(result.stdout, '');
});

test('unknown scenario is rejected before reading sources or invoking the worker', t => {
  const run = fixture(t, scalarModule, '#!/bin/sh\nprintf "unexpected worker invocation\\n" >&2\nexit 47\n');
  const result = run(['scenario', 'unknown-source-scenario', '<project>',
    '--entry', 'missing.ts#entry', '--source', 'missing.vue']);
  assert.equal(result.status, 2, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /SCENARIO_MODULE_INPUT: expected a supported scenario/);
  assert.doesNotMatch(result.stderr, /unexpected worker invocation|no such file/i);
});
