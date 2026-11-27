export function choose(values: readonly number[]): number {
  return values[values.length].length > 0 ? 7 : 11;
}
