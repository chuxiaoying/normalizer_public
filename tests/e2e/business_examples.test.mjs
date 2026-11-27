import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadFixtureEntries } from './fixtures.mjs';
import { assertPers } from './pers-schema.mjs';
import { assertRejection } from './rejection.mjs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const root = fileURLToPath(new URL('../../', import.meta.url));

for (const { seedId, entry, scene, language, cases } of loadFixtureEntries(root)) {
  for (const fixture of cases) {
    test([seedId, entry, fixture.name].join(' / '), t => {
      const project = mkdtempSync(join(tmpdir(), 'normalizer-example-'));
      t.after(() => rmSync(project, { recursive: true, force: true }));
      mkdirSync(join(project, 'src'));
      for (const [path, text] of Object.entries(fixture.input)) writeFileSync(join(project, path), text);
      const locator = fixture.entry;
      const args = scene ? ['scenario', scene, project, '--entry', locator, '--source', fixture.vueSource]
        : language === 'typescript' ? ['module', project, '--entry', locator]
        : ['source', project, locator];
      const run = spawnSync(join(root, 'implementation/tools/normalizer'), args, {
        cwd: root, encoding: 'utf8', timeout: 120_000,
      });
      if (run.error) throw run.error;
      assert.equal(run.signal, null, fixture.file + ': CLI terminated by signal');
      if (fixture.reject) {
        const { fragments, kind, location } = fixture.reject;
        assertRejection(run, location ?? locator, fragments, kind);
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
