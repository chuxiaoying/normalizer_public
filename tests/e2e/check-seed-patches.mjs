import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSeeds } from './catalog.mjs';
import { writePreparedSource } from './prepared-source.mjs';
import { readSeedBlocks } from './seed-example.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const extensions = new Map([['java', '.java'], ['vue', '.vue'], ['javascript', '.js'], ['typescript', '.ts']]);
let count = 0;

for (const [id, relative] of loadSeeds(root)) {
  let original;
  for (const block of readSeedBlocks(root, 'design/examples/' + relative)) {
    if (extensions.has(block.language)) {
      original = block;
      continue;
    }
    if (block.language !== 'diff') continue;
    const label = id + ':' + block.start;
    const path = block.body.match(/^--- a\/(.+)$/m)?.[1];
    assert.ok(original && path?.endsWith(extensions.get(original.language)), 'Missing matching original source: ' + label);
    const project = mkdtempSync(join(tmpdir(), 'normalizer-seed-patch-'));
    try {
      writePreparedSource(project, path, original.body, block.body);
    } catch (error) {
      throw new Error(label + ': ' + error.message, { cause: error });
    } finally {
      rmSync(project, { recursive: true, force: true });
    }
    original = undefined;
    count++;
  }
}

assert.ok(count > 0, 'No preprocessing patches discovered');
console.log('Seed preprocessing patches: ' + count + ' replayed');
