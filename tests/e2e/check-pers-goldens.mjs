import { fileURLToPath } from 'node:url';
import { applyExpectedChanges } from './business_example_series.mjs';
import { assertPers } from './pers-schema.mjs';
import { loadSeedEntries } from './seed-example.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
let checked = 0;
let failures = 0;
for (const { seedId, entry, fixture, variants, reject } of loadSeedEntries(root)) {
  for (const variant of [null, ...variants]) {
    if (variant?.reject ?? reject) continue;
    const label = [seedId, entry, variant?.name ?? 'original'].join(' / ');
    checked++;
    try {
      assertPers(applyExpectedChanges(fixture.expected, variant?.expect), label);
    } catch (error) {
      console.error(error.message);
      failures++;
    }
  }
}
console.log('PERS goldens: ' + checked + ' checked, ' + failures + ' invalid');
if (failures) process.exitCode = 1;
