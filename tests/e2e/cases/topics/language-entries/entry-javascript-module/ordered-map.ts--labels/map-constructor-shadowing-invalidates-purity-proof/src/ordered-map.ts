function Map(rows: readonly (readonly [string, string])[]) { return rows; }

export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ...(a === null ? [] : [['a', a] as const]),
    ...(b === null ? [] : [['b', b] as const]),
    ...(c === null ? [] : [['c', c] as const]),
  ]);
}
