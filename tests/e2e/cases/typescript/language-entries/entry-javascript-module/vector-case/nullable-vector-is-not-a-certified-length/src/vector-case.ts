export function choose(values: readonly number[] | null): number {
  return values.length > 0 ? 7 : 11;
}
