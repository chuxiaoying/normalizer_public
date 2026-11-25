import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { vueScenario } from '../../implementation/tools/scenarios/registry.mjs';

function record(value, name) {
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'Expected a mapping: ' + name);
  return Object.entries(value);
}

function readYaml(root, file) {
  return YAML.parse(readFileSync(join(root, file), 'utf8'));
}

function inventory(root, directory, include) {
  return readdirSync(join(root, directory), { recursive: true })
    .filter(include).sort();
}

function register(set, name, label) {
  assert.ok(!set.has(name), 'Duplicate ' + label + ': ' + name);
  set.add(name);
}

const examples = 'design/examples';

export function loadSeeds(root) {
  const seeds = new Map(record(readYaml(root, examples + '/seeds.yaml'), 'design seeds'));
  assert.ok(seeds.size > 0, 'The design seed catalog is empty');

  const documents = new Set();
  for (const [id, relative] of seeds) {
    assert.match(id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid seed ID');
    register(documents, relative, 'design document');
  }
  const published = inventory(root, examples, file => file.endsWith('.md') && !/(^|\/)(README|coverage|normal-form)\.md$/.test(file));
  assert.deepEqual([...documents].sort(), published, 'Design seed catalog must cover all example documents');

  return seeds;
}

export function loadSeedManifest(root, seeds = loadSeeds(root)) {
  const manifest = readYaml(root, examples + '/seed-manifest.yaml');
  assert.equal(manifest.version, 1, 'Unsupported seed manifest version');
  const topics = new Map(record(manifest.topics, 'topic registry'));
  const compositions = new Map(record(manifest.compositions, 'composition registry'));
  assert.ok(topics.size > 0, 'The topic registry is empty');
  assert.ok(compositions.size > 0, 'The composition registry is empty');
  for (const [id, entry] of [...topics, ...compositions]) {
    assert.match(id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid registry ID: ' + id);
    assert.ok(entry && typeof entry.path === 'string', 'Invalid registry entry: ' + id);
    assert.match(entry.path, /^(topics|compositions)\/[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid registry path: ' + id);
    assert.ok(existsSync(join(root, examples, entry.path, 'README.md')), 'Missing registry README: ' + id);
  }
  const topicIds = new Set(topics.keys());
  const compositionIds = new Set(compositions.keys());
  const records = record(manifest.seeds, 'seed metadata');
  assert.deepEqual(records.map(([id]) => id).sort(), [...seeds.keys()].sort(),
    'Seed metadata must cover every design seed exactly once');
  const metadata = new Map();
  const knownKinds = new Set(['topic', 'composition', 'basic']);
  for (const [id, value] of records) {
    assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'Invalid seed metadata: ' + id);
    assert.ok(knownKinds.has(value.kind), 'Invalid seed kind for ' + id);
    assert.match(value.topic, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid seed topic for ' + id);
    if (value.kind === 'composition') assert.ok(compositionIds.has(value.topic), 'Unknown composition topic for ' + id);
    else assert.ok(topicIds.has(value.topic), 'Unknown topic for ' + id);
    const registry = value.kind === 'composition' ? compositions : topics;
    const directory = registry.get(value.topic).path;
    assert.ok(seeds.get(id).startsWith(directory + '/'), 'Design seed must live in its registered topic: ' + id);
    const related = value.related ?? [];
    assert.ok(Array.isArray(related), 'Seed related topics must be a list: ' + id);
    assert.equal(new Set(related).size, related.length, 'Duplicate related topic: ' + id);
    assert.ok(!related.includes(value.topic), 'Seed topic cannot relate to itself: ' + id);
    for (const topic of [value.topic, ...related]) {
      assert.match(topic, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid related topic for ' + id);
      if (topic !== value.topic || value.kind !== 'composition') {
        assert.ok(topicIds.has(topic), 'Unknown related topic for ' + id);
      }
    }
    if (value.kind === 'composition') {
      assert.ok(related.length >= 2, 'Composition seed needs at least two related topics: ' + id);
    }
    metadata.set(id, { kind: value.kind, topic: value.topic, related });
  }
  return metadata;
}

export function loadSeedPlan(root, seeds = loadSeeds(root), seedMetadata = loadSeedManifest(root, seeds)) {
  const plan = readYaml(root, examples + '/seed-plan.yaml');
  assert.equal(plan.version, 1, 'Unsupported seed plan version');
  assert.ok(plan.model && Number.isInteger(plan.model.total), 'Seed plan needs a total');
  const manifest = readYaml(root, examples + '/seed-manifest.yaml');
  const topicIds = new Set(Object.keys(manifest.topics ?? {}));
  const records = new Map();
  const addGroup = (kind, topic, group) => {
    assert.ok(group && Array.isArray(group.seeds), 'Seed plan group needs seeds: ' + topic);
    if (kind === 'composition') {
      assert.ok(Array.isArray(group.topics) && group.topics.length >= 2, 'Composition group needs related topics: ' + topic);
      assert.equal(new Set(group.topics).size, group.topics.length, 'Duplicate composition topic: ' + topic);
      for (const related of group.topics) assert.ok(topicIds.has(related), 'Unknown composition topic reference: ' + related);
    }
    for (const item of group.seeds) {
      assert.ok(item && typeof item.id === 'string', 'Seed plan entry needs an ID: ' + topic);
      assert.match(item.id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, 'Invalid seed plan ID: ' + item.id);
      assert.ok(!records.has(item.id), 'Duplicate seed plan ID: ' + item.id);
      assert.ok(['seeded', 'planned'].includes(item.status), 'Invalid seed plan status: ' + item.id);
      assert.ok(typeof item.focus === 'string' && item.focus.length > 0, 'Seed plan entry needs a focus: ' + item.id);
      if (item.status === 'seeded') assert.ok(['covered', 'pending'].includes(item.test_status), 'Seeded entry needs test status: ' + item.id);
      else assert.ok(item.test_status === undefined, 'Planned entry cannot have test status: ' + item.id);
      assert.ok(!Object.hasOwn(item, 'review_status'), 'Review evidence belongs outside the seed plan: ' + item.id);
      records.set(item.id, { kind, topic, status: item.status, testStatus: item.test_status, focus: item.focus });
    }
  };
  for (const [topic, group] of record(plan.topics, 'seed plan topics')) addGroup(topic === 'basic-contracts' ? 'basic' : 'topic', topic, group);
  for (const [topic, group] of record(plan.compositions, 'seed plan compositions')) addGroup('composition', topic, group);
  assert.equal(records.size, plan.model.total, 'Seed plan total does not match its entries');
  const seeded = [...records.values()].filter(item => item.status === 'seeded').length;
  const planned = records.size - seeded;
  assert.equal(seeded, plan.model.seeded, 'Seed plan seeded count does not match its entries');
  assert.equal(planned, plan.model.planned, 'Seed plan planned count does not match its entries');
  for (const [id, metadata] of seedMetadata) {
    const plannedSeed = records.get(id);
    assert.ok(plannedSeed, 'Design seed is missing from the denominator plan: ' + id);
    assert.equal(plannedSeed.status, 'seeded', 'Design seed must be marked seeded: ' + id);
    assert.equal(plannedSeed.kind, metadata.kind, 'Seed kind differs between manifest and plan: ' + id);
    assert.equal(plannedSeed.topic, metadata.topic, 'Seed topic differs between manifest and plan: ' + id);
  }
  for (const [id, plannedSeed] of records) {
    if (plannedSeed.status === 'seeded') assert.ok(seeds.has(id), 'Seed plan says seeded but no design seed exists: ' + id);
  }
  return records;
}

export function loadCases(root, seeds = loadSeeds(root)) {
  const seedMetadata = loadSeedManifest(root, seeds);
  const seedPlan = loadSeedPlan(root, seeds, seedMetadata);
  const variants = 'tests/e2e/variants';
  const catalog = readYaml(root, 'tests/e2e/catalog.yaml');
  assert.deepEqual(Object.keys(catalog).sort(), ['groups', 'pending', 'version'], 'Unknown or missing test catalog section');
  assert.equal(catalog.version, 1, 'Unsupported test catalog version');
  const assigned = new Set();
  const series = new Set();
  const cases = [];
  for (const [group, bindings] of record(catalog.groups, 'test groups')) {
    assert.match(group, /^[a-z0-9-]+(?:\/[a-z0-9-]+)*$/, 'Invalid test group');
    const bindingsInGroup = record(bindings, group);
    assert.ok(bindingsInGroup.length, 'Empty test group: ' + group);
    for (const [seedId, binding] of bindingsInGroup) {
      assert.ok(seeds.has(seedId), 'Unknown design seed: ' + seedId);
      register(assigned, seedId, 'seed assignment');
      let entries = binding;
      let scene;
      if (group.split('/')[0] === 'vue') {
        assert.deepEqual(Object.keys(binding ?? {}).sort(), ['entries', 'scene'], 'Vue seed requires explicit entries and scene: ' + seedId);
        ({ entries, scene } = binding);
        assert.ok(vueScenario(scene),
          'Unknown or incompatible Vue CLI scene: ' + scene);
      }
      assert.ok(Array.isArray(entries) && entries.length, 'No test entry for seed: ' + seedId);
      const document = examples + '/' + seeds.get(seedId);
      for (const entry of entries) {
        const javascript = group.split('/')[0] === 'javascript';
        assert.match(entry, javascript
          ? /^[\w-]+\.js#[\w$]+$/
          : /^[\w$]+(?:[.#][\w$]+)*$/, 'Invalid seed entry');
        const [language] = group.split('/');
        const primaryTopic = seedMetadata.get(seedId).topic;
        const directory = group === language + '/' + primaryTopic ? group + '/' + seedId : group;
        const filename = directory + '/' + (javascript ? entry.split('#')[0].slice(0, -3) : entry.replaceAll('#', '-')) + '.yaml';
        register(series, filename, 'variant series');
        cases.push({
          seedId, document, entry, group, scene, seriesFile: variants + '/' + filename,
          ...seedMetadata.get(seedId), planStatus: seedPlan.get(seedId).status,
        });
      }
    }
  }

  assert.ok(cases.length, 'No registered CLI test entries');
  assert.ok(Array.isArray(catalog.pending), 'Expected a list of pending seeds');
  for (const id of catalog.pending) {
    assert.ok(seeds.has(id), 'Unknown pending seed: ' + id);
    register(assigned, id, 'seed assignment');
  }
  const registered = new Set(cases.map(item => item.seedId));
  for (const [id, plan] of seedPlan) {
    if (plan.status !== 'seeded') continue;
    const expected = registered.has(id) ? 'covered' : 'pending';
    assert.equal(plan.testStatus, expected, 'Seed plan test status disagrees with the CLI catalog: ' + id);
  }
  assert.deepEqual([...assigned].sort(), [...seeds.keys()].sort(), 'Every seed needs a test binding or an explicit pending status');
  const files = inventory(root, variants, file => /\.ya?ml$/i.test(file));
  assert.deepEqual([...series].sort(), files, 'Registered entries and variant files must correspond one to one');
  return cases;
}
