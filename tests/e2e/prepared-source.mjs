import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

export function writePreparedSource(project, sourceFile, source, patch) {
  assert.match(sourceFile, /^src\/[\w-]+\.(?:java|vue|js|ts)$/);
  const path = join(project, sourceFile);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, source);
  if (!patch) return;
  const applied = spawnSync('git', ['apply', '--whitespace=nowarn', '-'], {
    cwd: project, input: patch, encoding: 'utf8', timeout: 120_000,
  });
  if (applied.error) throw applied.error;
  assert.equal(applied.status, 0, 'Documented preprocessing diff: ' + applied.stderr);
  // Vue preprocessing renames the module; keep the original for attribution.
  if (sourceFile.endsWith('.vue')) writeFileSync(path, source);
}
