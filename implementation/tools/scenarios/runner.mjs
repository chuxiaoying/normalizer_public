import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { invokeKernel } from '../frontend/invoke-kernel.mjs';
import { buildModuleProject } from './module-project.mjs';
import { entryLocator } from '../frontend/entry-locator.mjs';

const [kernel, scene, root, entryFlag, entry, sourceFlag, original, ...extra] = process.argv.slice(2);
let directory;
try {
  const fail = message => { throw new Error('SCENARIO_MODULE_INPUT: ' + message); };
  if (entryFlag !== '--entry' || sourceFlag !== '--source' || !original || extra.length) {
    fail('expected --entry path#export --source original.vue');
  }
  const {file, name} = entryLocator(entry, fail);
  directory = mkdtempSync(join(tmpdir(), 'normalizer-scenario-'));
  const project = buildModuleProject({scene, root, file, name, original, directory});
  const result = invokeKernel(kernel, ['--project', project.projectPath, project.entry]);
  if (result.status !== 0) {
    const detail = result.stderr  ;
    throw new Error(`SCENARIO_KERNEL_FAILED: ${detail.trim()}`);
  }
  process.stdout.write(result.stdout);
} catch (error) {
  process.stderr.write(error.message + '\n');
  process.exitCode = 2;
} finally {
  if (directory) rmSync(directory, { recursive: true, force: true });
}
