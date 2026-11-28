import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));

function run(args, env = {}) {
  const result = spawnSync(join(root, 'implementation/tools/normalizer'), args, {
    cwd: root, env: { ...process.env, ...env }, encoding: 'utf8',
    timeout: 120_000, maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.signal, null);
  return result;
}

function project(t, files) {
  const path = mkdtempSync(join(tmpdir(), 'normalizer-cli-contract-'));
  t.after(() => rmSync(path, { recursive: true, force: true }));
  mkdirSync(join(path, 'src'));
  for (const [file, text] of Object.entries(files)) writeFileSync(join(path, 'src', file), text);
  return path;
}

function failure(result, code, kind, exit = 1) {
  assert.equal(result.status, exit, result.stderr);
  assert.equal(result.stdout, '');
  const diagnostic = JSON.parse(result.stderr);
  assert.equal(diagnostic.schema, 'normalizer.diagnostic/1');
  assert.equal(diagnostic.code, code);
  assert.equal(diagnostic.kind, kind);
  assert.equal(diagnostic.result, 'not-produced');
  return diagnostic;
}

const json = '--diagnostic-format=json';

test('dispatch failures are structured and preserve quotes and control characters', () => {
  const command = 'unknown "command" \\foo\nbar\t';
  const diagnostic = failure(run([json, command]), 'NORM-USAGE', 'USAGE', 2);
  assert.deepEqual(diagnostic.details, ['unknown command: ' + command]);
});

test('bootstrap failures are structured before the kernel can start', t => {
  const path = project(t, {});
  failure(run([json, 'source', path, 'src/Entry.java#Entry.run'], {
    TMPDIR: join(path, 'missing'),
  }), 'NORM-BOOTSTRAP-IO', 'ENVIRONMENT');
});

test('a Java business throw succeeds, unlike a reachable call without source', t => {
  const path = project(t, { 'Entry.java': `class Entry {
    static int businessFailure() { throw new IllegalArgumentException(); }
    static native int read();
    static int blocked(boolean readIt) { if (readIt) return read(); return 7; }
    static int unblocked() { if (false) return read(); return 7; }
  }` });
  const command = name => run([json, 'source', path, 'src/Entry.java#Entry.' + name]);
  const business = command('businessFailure');
  assert.equal(business.status, 0, business.stderr);
  assert.match(business.stdout, /throws:[\s\S]*IllegalArgumentException/);
  assert.doesNotMatch(business.stdout, /^R:/m);
  const blocked = failure(command('blocked'), 'NORM-OUT-OF-SOURCE', 'ABORT');
  assert.match(blocked.location, /Entry\.blocked/);
  assert.equal(blocked.target, 'Entry.read');
  assert.match(blocked.details.join('\n'), /Panic: not-support/);
  assert.doesNotMatch(blocked.details.join('\n'), /ENGINE-FAILURE|panicked at/);
  const unblocked = command('unblocked');
  assert.equal(unblocked.status, 0, unblocked.stderr);
  assert.match(unblocked.stdout, /^R: '7'$/m);
});

test('a missing TypeScript file reports a typed location', t => {
  const path = project(t, {});
  const diagnostic = failure(run([json, 'module', path, '--entry', 'src/missing.ts#run']),
    'NORM-INPUT-IO', 'INPUT');
  assert.match(diagnostic.location, /src[/\\]missing\.ts$/);
});

test('TypeScript parser and checker evidence receive admission codes and positions', t => {
  const path = project(t, {
    'syntax.ts': 'export function run(: number { return 1; }',
    'type.ts': 'export function run(): number { return "text"; }',
    'host.ts': 'export function run(): string { return document.title; }',
  });
  const command = file => run([json, 'module', path, '--entry', 'src/' + file + '.ts#run']);
  const syntax = failure(command('syntax'), 'NORM-TS-SYNTAX', 'PREPROCESS_REQUIRED');
  assert.equal(syntax.stage, 'admission');
  assert.match(syntax.location, /syntax\.ts:1:[0-9]+$/);
  assert.match(syntax.tool_code, /^TS[0-9]+$/);
  const type = failure(command('type'), 'NORM-TS-TYPECHECK', 'PREPROCESS_REQUIRED');
  assert.equal(type.tool_code, 'TS2322');
  assert.match(type.details.join('\n'), /not assignable/);
  const host = failure(command('host'), 'NORM-TS-TYPECHECK', 'PREPROCESS_REQUIRED');
  assert.equal(host.tool_code, 'TS2584');
  assert.match(host.details.join('\n'), /Host libraries are not loaded/);
  assert.doesNotMatch(host.details.join('\n'), /include ['"]dom/);
});

test('explicit, implicit and resolved TypeScript any stop before execution', t => {
  const path = project(t, {
    'explicit.ts': 'function helper(value: any) { return value; } export function run(): number { return 1; }',
    'implicit.ts': 'function helper(value) { return value; } export function run(): number { return 1; }',
    'resolved.ts': 'function helper() { return JSON.parse("1"); } export function run(): number { return 1; }',
  });
  for (const [file, tool] of [['explicit', undefined], ['implicit', 'TS7006'], ['resolved', undefined]]) {
    const result = run([json, 'module', path, '--entry', 'src/' + file + '.ts#run']);
    const diagnostic = failure(result, 'NORM-TS-ANY', 'PREPROCESS_REQUIRED');
    assert.equal(diagnostic.stage, 'admission');
    assert.equal(diagnostic.tool_code, tool);
    assert.match(diagnostic.details.join('\n'), new RegExp('Panic: not-support;.*' + file));
  }
});

test('a TypeScript scenario rejects compiler suppression in preflight', t => {
  const path = project(t, {
    'page.ts': '// @ts-nocheck\n/** @N componentProp Card#total.title */\nexport function run(): string { return "Title"; }',
  });
  const result = run([json, 'scenario', 'frontend-vo-consumption', path, '--entry', 'src/page.ts#run']);
  const diagnostic = failure(result, 'NORM-TS-CHECK-DIRECTIVE', 'PREPROCESS_REQUIRED');
  assert.match(diagnostic.location, /page\.ts:1:1$/);
  assert.match(diagnostic.details.join('\n'), /cannot disable or suppress/);
});

test('large TypeScript PERS is not limited by Node child-process buffering', t => {
  const literal = 'a'.repeat(1024 * 1024 + 32768);
  const path = project(t, { 'main.ts': 'export function run(): string { return "' + literal + '"; }' });
  const result = run(['module', path, '--entry', 'src/main.ts#run']);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(result.stdout.includes(literal), 'the full string must be present, not just a prefix');
});

test('checked-in error documentation matches the live CLI catalog', () => {
  const result = run(['errors']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, readFileSync(join(root, 'implementation/docs/cli-error-codes.md'), 'utf8'));
});
