import { applyExpectedChanges, resolveVariantEdits } from '../../../tests/e2e/business_example_series.mjs';
import { loadSeedManifest, loadSeeds } from '../../../tests/e2e/catalog.mjs';
import { loadSeedEntries } from '../../../tests/e2e/seed-example.mjs';

export function loadSeedTests(root) {
  const seeds = loadSeeds(root);
  const metadata = loadSeedManifest(root, seeds);
  const documents = new Map();
  for (const [seedId, relative] of seeds) {
    documents.set('design/examples/' + relative, { seedId, ...metadata.get(seedId), entries: [] });
  }
  for (const { document, entry, seriesFile, fixture, variants, reject, kind, topic, related } of loadSeedEntries(root, seeds)) {
    const tests = [null, ...variants].map(variant => {
      const changes = variant?.expect ?? [];
      const fragments = variant?.reject ?? reject ?? null;
      return {
        name: variant?.name ?? 'original',
        edits: resolveVariantEdits(fixture.mainFile, variant?.edits ?? []),
        changes,
        expected: fragments ? null : applyExpectedChanges(fixture.expected, changes),
        reject: fragments,
        rejection: fixture.rejection,
        inheritsRejection: Boolean(variant && reject && !variant.reject),
      };
    });
    const seed = documents.get(document);
    seed.kind = kind;
    seed.topic = topic;
    seed.related = related;
    seed.entries.push({ entry, seriesFile, tests });
  }
  return documents;
}
