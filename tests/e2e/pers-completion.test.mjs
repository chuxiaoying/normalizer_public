import assert from 'node:assert/strict';
import { test } from 'node:test';
import { hasGuaranteedThrows } from './pers-completion.mjs';

function throwsCover(guards, { parameters = {}, E = {} } = {}) {
  return hasGuaranteedThrows({ P: { parameters }, E, throws: guards.map(when => ({ when })) });
}

test('a typed non-null P or E array has a natural length', () => {
  const guards = ['LENGTH(values) > 0', 'LENGTH(values) == 0.00'];
  assert.ok(throwsCover(guards, { parameters: { values: { type: 'number?[]' } } }));
  assert.ok(throwsCover(guards, { E: { values: { type: 'number[]' } } }));
  assert.ok(throwsCover(['LENGTH(E.values) >= -0.00'], { E: { values: { type: 'number[]' } } }));
  assert.ok(throwsCover(['LENGTH(P.values) != 0.5'], { parameters: { values: { type: 'number[]' } } }));
});

test('length proof does not guess type, nullability, identity or coverage', () => {
  const guards = ['LENGTH(values) > 0', 'LENGTH(values) == 0'];
  assert.equal(throwsCover(guards), false);
  for (const type of ['number[]?', 'number', 'text']) {
    assert.equal(throwsCover(guards, { E: { values: { type } } }), false);
  }
  const arrays = { values: { type: 'number[]' } };
  assert.equal(throwsCover(guards, { parameters: arrays, E: arrays }), false);
  assert.equal(throwsCover(['LENGTH(values) > 0'], { E: arrays }), false);
  assert.equal(throwsCover(['LENGTH(values) > 2', 'LENGTH(values) == 2'], { E: arrays }), false);
  assert.equal(throwsCover(['LENGTH(P.values) > 0', 'LENGTH(E.values) == 0'], { parameters: arrays, E: arrays }), false);
  assert.equal(throwsCover(['values > 0', 'values == 0'], { E: { values: { type: 'number' } } }), false);
});
