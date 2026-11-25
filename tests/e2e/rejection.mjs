import assert from 'node:assert/strict';

const header = /^(?:\[([A-Z][A-Z0-9-]*)\]|([A-Z][A-Z0-9_]*):)(?:\s|$)/;
const wrapper = /^\[PIPELINE-RESOLVE\]\s+/;
const panic = /^thread '.+'(?: \(\d+\))? panicked at /;

export function diagnosticCode(marker) {
  return /^\[([A-Z][A-Z0-9-]*)\]$/.exec(marker)?.[1] ??
    /^([A-Z][A-Z0-9_]*):$/.exec(marker)?.[1];
}

export function assertRejection({ status, stdout, stderr }, entry, fragments, kind) {
  assert.ok(Number.isInteger(status) && status > 0, entry + ': expected a failing CLI exit');
  assert.equal(stdout, '', entry + ': rejection must not emit PERS');
  const [first, second] = stderr.trimStart().split('\n');
  const primary = (panic.test(first) ? second ?? '' : first).replace(wrapper, '');
  const identity = header.exec(primary);
  const code = diagnosticCode(fragments[0]);
  assert.ok(code, entry + ': expected rejection must name a diagnostic code');
  assert.equal(identity?.[1] ?? identity?.[2], code, entry + ': wrong primary diagnostic\n' + stderr);
  const inner = code === 'SCENARIO_KERNEL_FAILED' ? primary.replace(/^SCENARIO_KERNEL_FAILED:\s+/, '') : primary;
  if (inner !== primary) {
    assert.equal(header.exec(inner)?.[1], diagnosticCode(fragments[1]), entry + ': wrong nested kernel diagnostic\n' + stderr);
    assert.ok(diagnosticCode(fragments[1]), entry + ': expected rejection must name the nested kernel diagnostic');
  }
  const expectedKind = kind ?? fragments.find(fragment => fragment.startsWith('kind='))?.slice(5);
  if (expectedKind) {
    assert.equal(/(?:^|\s)kind=([A-Z_]+)(?:\s|$)/.exec(inner)?.[1], expectedKind, entry + ': wrong diagnostic kind\n' + stderr);
  }
  for (const fragment of fragments.slice(1)) {
    assert.ok(stderr.includes(fragment), entry + ': missing ' + JSON.stringify(fragment) + '\n' + stderr);
  }
}
