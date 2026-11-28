import { loadSeedManifest, loadSeeds, seedDirectory } from '../../../tests/e2e/harness/catalog.mjs';
import { loadFixtureEntries } from '../../../tests/e2e/harness/fixtures.mjs';

export function loadSeedTests(root) {
  const seeds = loadSeeds(root);
  const metadata = loadSeedManifest(root, seeds);
  const documents = new Map();
  for (const [seedId, relative] of seeds) {
    documents.set(seedDirectory + '/' + relative, { seedId, ...metadata.get(seedId), entries: [] });
  }
  for (const { document, entry, cases } of loadFixtureEntries(root, seeds)) {
    documents.get(document).entries.push({ entry, entryPageFile: cases[0].file, tests: cases });
  }
  return documents;
}
