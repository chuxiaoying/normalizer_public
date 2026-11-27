export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
  return Object.freeze([
    ...(a === null ? [] : [['a', a] as const]),
    ...(b === null ? [] : [['b', b] as const]),
    ...(c === null ? [] : [['c', c] as const]),
  ]);
}
