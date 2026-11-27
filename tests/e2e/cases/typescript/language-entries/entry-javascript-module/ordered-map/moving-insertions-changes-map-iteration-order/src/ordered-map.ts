export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ...(b === null ? [] : [['10', b] as const]),
    ...(a === null ? [] : [['2', a] as const]),
  ]);
}
