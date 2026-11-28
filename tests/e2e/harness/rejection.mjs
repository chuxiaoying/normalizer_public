import assert from 'node:assert/strict';

const header = /^\[([A-Z][A-Z0-9-]*)\] kind=([A-Z_]+) stage=([a-z]+)\s*$/;

export function diagnosticCode(marker) {
  return /^\[([A-Z][A-Z0-9-]*)\]$/.exec(marker)?.[1];
}

export function assertRejection({ status, stdout, stderr }, entry, fragments, kind, location) {
  assert.ok(Number.isInteger(status) && status > 0, entry + ': expected a failing CLI exit');
  assert.equal(stdout, '', entry + ': rejection must not emit PERS');
  const block = stderr.trim();
  const lines = block.split('\n');
  const primary = header.exec(lines[0] ?? '');
  const code = diagnosticCode(fragments[0]);
  assert.ok(code, entry + ': expected rejection must name a diagnostic code');
  assert.equal(primary?.[1], code, entry + ': wrong primary diagnostic\n' + stderr);
  assert.equal(lines.filter(line => header.test(line)).length, 1, entry + ': expected one diagnostic envelope');
  assert.ok(lines.includes('result=not-produced'), entry + ': missing failed outcome');
  if (code !== 'NORM-ENGINE-FAILURE') {
    assert.doesNotMatch(block, /^thread '.+'(?: \(\d+\))? panicked at /m, entry + ': expected errors are not engine panics');
    assert.ok(!lines.some(line => line.startsWith('[NORM-ENGINE-FAILURE]')), entry + ': expected errors are not engine failures');
  }
  const expectedKind = kind ?? fragments.find(fragment => fragment.startsWith('kind='))?.slice(5);
  if (expectedKind) assert.equal(primary[2], expectedKind, entry + ': wrong diagnostic kind\n' + stderr);
  const requiredLocation = location ?? (kind ? entry : undefined);
  if (requiredLocation) assert.ok(block.includes(requiredLocation), entry + ': missing location ' + requiredLocation + '\n' + stderr);
  for (const fragment of fragments.slice(1)) {
    assert.ok(block.includes(fragment), entry + ': missing ' + JSON.stringify(fragment) + ' in diagnostic\n' + stderr);
  }
}
