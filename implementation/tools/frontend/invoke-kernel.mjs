import { spawnSync } from 'node:child_process';

export function invokeKernel(kernel, args, input) {
  const result = spawnSync(kernel, args, { encoding: 'utf8', input });
  if (result.error) throw result.error;
  return result;
}
