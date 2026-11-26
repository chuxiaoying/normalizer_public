import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const cli = join(root, 'implementation/tools/normalizer');
const scalarModule = '/** @returns {number} */\nexport function value() { return 7; }\n';

function fixture(t, source, worker) {
  const directory = mkdtempSync(join(tmpdir(), 'normalizer-frontend-transport-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const target = join(directory, 'target');
  const project = join(directory, 'project');
  mkdirSync(join(target, 'debug'), { recursive: true });
  mkdirSync(project);
  writeFileSync(join(target, 'debug/normalizer-normalized'), worker, { mode: 0o700 });
  writeFileSync(join(project, 'value.js'), source);
  return () => {
    const result = spawnSync(cli, ['module', project, 'value.js', 'value'], {
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
  assert.match(result.stderr, /\[NORM-JS-MODULE\]/);
  assert.doesNotMatch(result.stderr, /unexpected worker invocation/);
});

test('worker failure keeps its status and diagnostic but discards incomplete output', t => {
  const run = fixture(t, scalarModule, '#!/bin/sh\nprintf "incomplete result\\n"\nprintf "worker diagnostic\\n" >&2\nexit 47\n');
  const result = run();
  assert.equal(result.status, 47, result.stderr);
  assert.equal(result.stderr, 'worker diagnostic\n');
  assert.equal(result.stdout, '');
});
