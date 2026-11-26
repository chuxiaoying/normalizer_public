import assert from 'node:assert/strict';
import { diagnosticCode } from './rejection.mjs';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

function checkKeys(object, allowed) {
  for (const key of Object.keys(object)) assert.ok(allowed.includes(key), 'Unexpected fixture key: ' + key);
}

export function loadSeries(file) {
  const series = YAML.parse(readFileSync(file, 'utf8'));
  let variants = series;
  let reject;
  if (!Array.isArray(series)) {
    assert.deepEqual(Object.keys(series ?? {}).sort(), ['reject', 'variants'], 'Invalid rejection series: ' + file);
    ({ reject, variants } = series);
    assert.ok(Array.isArray(reject) && reject.length && reject.every(text => typeof text === 'string' && text.trim().length),
      'Expected diagnostic fragments: ' + file);
    assert.ok(diagnosticCode(reject[0]), 'Expected primary diagnostic marker: ' + file);
  }
  assert.ok(Array.isArray(variants) && variants.length, 'Empty series: ' + file);
  const names = new Set(['original']);
  for (const variant of variants) {
    checkKeys(variant, ['name', 'edits', 'expect', 'reject']);
    assert.match(variant.name, /^[a-z0-9]+(?:-[a-z0-9]+)*$/);
    assert.ok(!names.has(variant.name), 'Duplicate case: ' + variant.name);
    names.add(variant.name);
    assert.ok(Array.isArray(variant.edits) && variant.edits.length, 'Missing input mutation');
    if (Object.hasOwn(variant, 'expect')) assert.ok(Array.isArray(variant.expect) && variant.expect.length);
    if (Object.hasOwn(variant, 'reject')) {
      assert.ok(!variant.expect && Array.isArray(variant.reject) && variant.reject.length);
      assert.ok(variant.reject.every(text => typeof text === 'string' && text.trim().length));
      assert.ok(diagnosticCode(variant.reject[0]), 'Expected primary diagnostic marker: ' + file + ' / ' + variant.name);
    }
  }
  return { variants, reject };
}

export function resolveVariantEdits(mainFile, edits) {
  return edits.map(edit => {
    checkKeys(edit, ['file', 'from', 'to', 'create']);
    const file = edit.file ?? mainFile;
    assert.match(file, /^src\/[\w-]+\.(?:java|vue|mjs|js|ts)$/);
    if (Object.hasOwn(edit, 'create')) {
      assert.ok(Object.hasOwn(edit, 'file') && !Object.hasOwn(edit, 'from') && !Object.hasOwn(edit, 'to') &&
        typeof edit.create === 'string' && edit.create.length, 'Create requires a new relative source file and full contents');
    } else {
      assert.ok(typeof edit.from === 'string' && edit.from.length && typeof edit.to === 'string' && edit.from !== edit.to);
    }
    return { ...edit, file };
  });
}

// Each case starts with the original prepared project and independent document oracle.
// Replacements must identify exactly one source location so stale fixtures fail visibly.
export function applyVariant(project, mainFile, baseline, variant) {
  for (const edit of resolveVariantEdits(mainFile, variant.edits)) {
    const { file } = edit;
    const path = join(project, file);
    if (Object.hasOwn(edit, 'create')) {
      writeFileSync(path, edit.create, { encoding: 'utf8', flag: 'wx' });
      continue;
    }
    const source = readFileSync(path, 'utf8');
    const start = source.indexOf(edit.from);
    assert.ok(start >= 0 && source.lastIndexOf(edit.from) === start, variant.name + ': expected one occurrence of ' + JSON.stringify(edit.from) + ' in ' + file);
    writeFileSync(path, source.slice(0, start) + edit.to + source.slice(start + edit.from.length));
  }
  return applyExpectedChanges(baseline, variant.expect);
}

export function applyExpectedChanges(baseline, changes = []) {
  const expected = structuredClone(baseline);
  for (const change of changes) {
    checkKeys(change, ['path', 'value', 'absent', 'add']);
    const operation = ['value', 'absent', 'add'].filter(key => Object.hasOwn(change, key));
    assert.ok(Array.isArray(change.path) && change.path.length && operation.length === 1 &&
      (operation[0] !== 'absent' || change.absent === true));
    for (const key of change.path) {
      assert.ok((typeof key === 'string' && key.length && !['__proto__', 'constructor', 'prototype'].includes(key)) ||
        (Number.isSafeInteger(key) && key >= 0), 'Invalid expected field path');
    }
    let node = expected;
    for (const key of change.path.slice(0, -1)) {
      assert.ok(Object.hasOwn(node, key), 'Unknown expected field: ' + change.path.join('.'));
      node = node[key];
    }
    const last = change.path.at(-1);
    if (operation[0] === 'add') {
      assert.ok(node && typeof node === 'object' && !Array.isArray(node) && typeof last === 'string', 'Add requires a mapping field');
      assert.ok(!Object.hasOwn(node, last), 'Expected field already exists: ' + change.path.join('.'));
      node[last] = change.add;
      continue;
    }
    assert.ok(Object.hasOwn(node, last), 'Unknown expected field: ' + change.path.join('.'));
    if (operation[0] === 'value') {
      node[last] = change.value;
    } else {
      assert.ok(!Array.isArray(node), 'Use a whole-array replacement to remove a list item');
      delete node[last];
    }
  }
  return expected;
}
