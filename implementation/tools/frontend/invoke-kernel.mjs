import { spawnSync } from 'node:child_process';

export function invokeKernel(kernel, args, input) {
  const result = spawnSync(kernel, args, { encoding: 'utf8', input });
  if (result.error) throw result.error;
  if (result.status === null) {
    const signal = result.signal ? ` (${result.signal})` : '';
    throw new Error(`[NORM-ANALYSIS-INCOMPLETE] kernel stopped before producing a complete result${signal}`);
  }
  return result;
}
