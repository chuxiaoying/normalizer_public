export function labels(a: string | null, b: string | null, c: string | null, d: string | null, e: string | null, f: string | null, g: string | null, h: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ...(a === null ? [] : [['a', a] as const]),
    ...(b === null ? [] : [['b', b] as const]),
    ...(c === null ? [] : [['c', c] as const]),
    ...(d === null ? [] : [['d', d] as const]),
    ...(e === null ? [] : [['e', e] as const]),
    ...(f === null ? [] : [['f', f] as const]),
    ...(g === null ? [] : [['g', g] as const]),
    ...(h === null ? [] : [['h', h] as const]),
  ]);
}
