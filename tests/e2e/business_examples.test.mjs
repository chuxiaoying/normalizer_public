import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadFixtureEntries } from './harness/fixtures.mjs';
import { assertPers } from './harness/pers-schema.mjs';
import { assertRejection } from './harness/rejection.mjs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const root = fileURLToPath(new URL('../../', import.meta.url));

for (const { seedId, entry, scene, language, cases } of loadFixtureEntries(root)) {
  for (const fixture of cases) {
    test([seedId, entry, fixture.name].join(' / '), t => {
      const project = mkdtempSync(join(tmpdir(), 'normalizer-example-'));
      t.after(() => rmSync(project, { recursive: true, force: true }));
      cpSync(join(root, fixture.directory, 'src'), join(project, 'src'), { recursive: true });
      const locator = fixture.entry;
      const args = scene ? ['scenario', scene, project, '--entry', locator]
        : language === 'typescript' ? ['module', project, '--entry', locator]
        : ['source', project, locator];
      const run = spawnSync(join(root, 'implementation/tools/normalizer'), args, {
        cwd: root, encoding: 'utf8', timeout: 120_000,
      });
      if (run.error) throw run.error;
      assert.equal(run.signal, null, fixture.file + ': CLI terminated by signal');
      if (fixture.reject) {
        const { fragments, kind, location } = fixture.reject;
        assertRejection(run, locator, fragments, kind, location);
      } else {
        assert.equal(run.status, 0, run.stderr.trim() || 'Normalizer failed without a diagnostic');
        const actual = YAML.parse(run.stdout);
        assertPers(fixture.expected, fixture.file + ' expected');
        assertPers(actual, fixture.file + ' actual');
        assert.deepEqual(actual, fixture.expected);
      }
    });
  }
}
