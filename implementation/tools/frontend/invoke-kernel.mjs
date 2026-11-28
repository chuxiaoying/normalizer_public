import { spawnSync } from 'node:child_process';
import { DiagnosticFailure } from '../diagnostics/internal.mjs';

// The public CLI owns the output transaction. Do not buffer kernel stdout
// in Node (spawnSync's default maxBuffer made valid large PERS fail).
export function invokeKernel(kernel, args, input) {
  const result = spawnSync(kernel, args, {
    stdio: [input === undefined ? 'ignore' : 'pipe', 'inherit', 'inherit'],
    ...(input === undefined ? {} : {input}),
  });
  if (result.error) throw new DiagnosticFailure('NORM-RESULT-TRANSPORT', result.error.message);
  if (result.status === null) {
    throw new DiagnosticFailure('NORM-INTERRUPTED', 'kernel stopped before producing a complete result'
      + (result.signal ? ' (' + result.signal + ')' : ''));
  }
  return result.status;
}
