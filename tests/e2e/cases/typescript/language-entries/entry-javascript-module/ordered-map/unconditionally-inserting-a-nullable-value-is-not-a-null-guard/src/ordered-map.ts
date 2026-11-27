export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ...(a === null ? [] : [['a', a] as const]),
    ['b', b],
    ...(c === null ? [] : [['c', c] as const]),
  ]);
}
