import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { assertPers } from './pers-schema.mjs';
import { assertRejection } from './rejection.mjs';
import { applyExpectedChanges, explicitVariantEntry } from './business_example_series.mjs';

const base = {
  schema: 'normalizer.pers/1',
  entry: { function: 'src/Example.java#Example.run', returns: 'number' },
  P: { parameters: {} },
  R: '0',
};
const call = (result, args = []) => ({ call: { external: 'port.read', args, returns: 'number', result } });

test('fixture entry overrides require a canonical Java source path and member chain', () => {
  assert.equal(explicitVariantEntry({}), undefined);
  assert.equal(explicitVariantEntry({entry: 'src/Example.java#Example.INSTANCE.run'}), 'src/Example.java#Example.INSTANCE.run');
  for (const entry of ['../Example.java#Example.run', 'src/Example.java#run', 'src/Example.java#Example.run(1)', 'src/example.ts#run']) {
    assert.throws(() => explicitVariantEntry({entry}), /Entry override/);
  }
});

test('PERS requires receipt references to be bound', () => {
  assert.throws(() => assertPers({ ...base, R: '$e_9' }, 'unbound'), /unbound.*\$e_9/i);
  assert.throws(() => assertPers({ ...base, throws: [{ type: 'Failure', when: 'completion.is_throw($e_9, Failure)' }] }, 'throw'), /unbound.*\$e_9/i);
});
test('PERS requires receipts to precede use in effects and forbids rebinding', () => {
  assert.throws(() => assertPers({ ...base, S: [call('$e_0', ['$e_1']), call('$e_1')] }, 'forward'), /unbound.*\$e_1/i);
  assert.throws(() => assertPers({ ...base, S: [call('$e_0', ['$e_0'])] }, 'self'), /unbound.*\$e_0/i);
  assert.throws(() => assertPers({ ...base, S: [call('$e_0'), call('$e_0')] }, 'duplicate'), /duplicate.*\$e_0/i);
  assert.doesNotThrow(() => assertPers({ ...base, S: [call('$e_0'), call('$e_1', ['$e_0'])], R: '$e_1' }, 'ordered'));
  assert.doesNotThrow(() => assertPers({ ...base, R: '"not a receipt: \\"$e_9\\""' }, 'string literal'));
});
test('a variant can add a previously absent observable without weakening replace', () => {
  const throws = [{ type: 'Failure', when: 'true' }];
  assert.deepEqual(applyExpectedChanges(base, [{ path: ['throws'], add: throws }]), { ...base, throws });
  assert.throws(() => applyExpectedChanges(base, [{ path: ['throws'], value: throws }]), /Unknown expected field/);
  assert.throws(() => applyExpectedChanges(base, [{ path: ['R'], add: '5' }]), /already exists/);
  assert.throws(() => applyExpectedChanges(base, [{ path: ['R'], add: '5', value: '6' }]));
});

test('reject guards match the primary phase and diagnostic kind, not an incidental substring', () => {
  const fragments = ['[NORM-PROJECT]', 'kind=PREPROCESS_REQUIRED', 'undeclared query input b'];
  const verify = (stderr, stdout = '') => assertRejection({ status: 1, stderr, stdout }, 'JoinService.count', fragments);
  assert.throws(() => verify('[BOOTSTRAP-FAILED] undeclared query input b\n' + fragments.join(' ')), /wrong primary diagnostic/);
  assert.throws(() => verify('[NORM-PROJECT] kind=ABORT\nproblem=undeclared query input b'), /wrong diagnostic kind/);
  assert.throws(() => verify('[NORM-PROJECT] kind=PREPROCESS_REQUIRED\nproblem=unknown source'), /missing/);
  assert.throws(() => verify('[NORM-PROJECT] kind=PREPROCESS_REQUIRED\nproblem=unknown source\n[SECONDARY-FAILURE] undeclared query input b'), /missing.*primary diagnostic/);
  assert.throws(() => assertRejection({
    status: 1, stdout: '', stderr: '[NORM-PROJECT] kind=PREPROCESS_REQUIRED\nproblem=undeclared query input b\n[SECONDARY-FAILURE] JoinService.count',
  }, 'JoinService.count', fragments, 'PREPROCESS_REQUIRED'), /missing entry in primary diagnostic/);
  assert.doesNotThrow(() => verify('[NORM-PROJECT] kind=PREPROCESS_REQUIRED\nproblem=undeclared query input b'));
  assert.throws(() => verify('[NORM-PROJECT] kind=PREPROCESS_REQUIRED\nproblem=undeclared query input b', 'R: 0'), /must not emit PERS/);
});

test('original Vue handler uses the invocation date even if props change while awaiting', async () => {
  const source = readFileSync(new URL('../../design/examples/topics/language-entries/frontend-vo-production-vue.md', import.meta.url), 'utf8');
  const handler = /^async function onLoad\(\) \{\n[\s\S]*?^\}/m.exec(source)?.[0];
  assert.ok(handler, 'Expected the original Vue handler');
  const props = { accountId: 42, today: 10 };
  const arrears = { status: {}, summary: {} };
  const profile = Promise.withResolvers();
  const bills = Promise.withResolvers();
  const create = new Function('props', 'arrears', 'profileClient', 'arrearsClient', handler + '\nreturn onLoad;');
  const onLoad = create(props, arrears, { fetchProfile: () => profile.promise }, { queryOpenBills: () => bills.promise });
  const completion = onLoad();
  props.today = 11;
  profile.resolve({ accountId: 42, minPriority: 0, quota: 100 });
  bills.resolve([
    { dueDate: 10, priority: 1, amount: 4, waivedAmount: 0 },
    { dueDate: 11, priority: 1, amount: 7, waivedAmount: 0 },
  ]);
  await completion;
  assert.equal(arrears.summary.payable, 4);
  assert.equal(arrears.status.ready, true);
});
