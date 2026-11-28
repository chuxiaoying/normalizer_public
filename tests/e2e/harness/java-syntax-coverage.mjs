import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import YAML from '../../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { loadSeeds } from './catalog.mjs';

const index = 'design/examples/java-syntax-coverage.yaml';
const id = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

function record(value, at, keys) {
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), at + ': expected mapping');
  if (keys) {
    for (const key of Object.keys(value)) assert.ok(keys.includes(key), at + ': unknown key ' + key);
  }
  return Object.entries(value);
}

function describe(value, at) {
  assert.ok(typeof value === 'string' && value.trim(), at + ': expected nonempty description');
}

export function loadJavaSyntaxCoverage(root, entries, seeds = loadSeeds(root)) {
  const data = YAML.parse(readFileSync(join(root, index), 'utf8'));
  record(data, index, ['version', 'families']);
  assert.equal(data.version, 1, index + ': unsupported version');
  const fixtures = new Map();
  for (const { seedId, language, scene, entry, cases } of entries) {
    if (language !== 'java' || scene) continue;
    for (const fixture of cases) {
      fixtures.set(JSON.stringify([seedId, entry, fixture.name]), fixture.reject ? 'reject' : 'pers');
    }
  }

  const result = [];
  for (const [family, definition] of record(data.families, index)) {
    const at = index + ': ' + family;
    assert.match(family, id, at + ': invalid family ID');
    record(definition, at, ['title', 'leaves']);
    describe(definition.title, at);
    const leaves = record(definition.leaves, at);
    assert.ok(leaves.length, at + ': empty family');
    for (const [leaf, item] of leaves) {
      const label = at + '/' + leaf;
      assert.match(leaf, id, label + ': invalid leaf ID');
      record(item, label, ['focus', 'seed', 'gap', 'evidence']);
      describe(item.focus, label);
      if (Object.hasOwn(item, 'seed')) assert.ok(seeds.has(item.seed), label + ': unknown seed');
      assert.notEqual(Object.hasOwn(item, 'gap'), Object.hasOwn(item, 'evidence'),
        label + ': specify gap or evidence, not both');
      if (item.gap !== undefined) {
        assert.ok(['review', 'fixture', 'implementation', 'design'].includes(item.gap), label + ': invalid gap');
        result.push({ family, leaf, status: item.gap });
        continue;
      }

      assert.ok(seeds.has(item.seed), label + ': evidence requires a seed');
      assert.ok(Array.isArray(item.evidence) && item.evidence.length, label + ': empty evidence');
      const seen = new Set();
      for (const proof of item.evidence) {
        assert.ok(Array.isArray(proof) && proof.length === 3, label + ': expected [entry, case, outcome]');
        const [entry, name, outcome] = proof;
        assert.ok(typeof entry === 'string' && typeof name === 'string' && id.test(name),
          label + ': invalid fixture reference');
        assert.ok(['pers', 'reject'].includes(outcome), label + ': invalid outcome');
        const key = JSON.stringify([item.seed, entry, name]);
        assert.ok(!seen.has(key), label + ': duplicate evidence ' + key);
        seen.add(key);
        assert.equal(fixtures.get(key), outcome, label + ': missing Java fixture or wrong outcome ' + key);
      }
      result.push({ family, leaf, status: 'registered' });
    }
  }
  assert.ok(result.length, index + ': empty index');
  return result;
}
