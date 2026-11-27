export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ...(a === null ? [] : [['a', a] as const]),
    ...(b === null ? [] : [['b', b] as const]),
    ...(c === null ? [] : [['c', c] as const]), ...(c === null ? [] : [['d', c] as const]),
...(c === null ? [] : [['e', c] as const]), ...(c === null ? [] : [['f', c] as const]),
...(c === null ? [] : [['g', c] as const]), ...(c === null ? [] : [['h', c] as const]),
  ]);
}
