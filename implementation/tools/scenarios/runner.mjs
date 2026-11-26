import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { invokeKernel } from '../frontend/invoke-kernel.mjs';
import { buildModuleProject } from './module-project.mjs';

const [kernel, scene, root, entryFlag, entry, sourceFlag, original, ...extra] = process.argv.slice(2);
let directory;
try {
  const separator = entry?.lastIndexOf(':');
  directory = mkdtempSync(join(tmpdir(), 'normalizer-scenario-'));
  const project = buildModuleProject({
    scene, root, file: entry.slice(0, separator), name: entry.slice(separator + 1), original, directory,
  });
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
