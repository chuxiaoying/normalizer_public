type ReadonlyArray<T> = { readonly length: number };
export function choose(value: ReadonlyArray<number>): number {
  return value.length;
}
