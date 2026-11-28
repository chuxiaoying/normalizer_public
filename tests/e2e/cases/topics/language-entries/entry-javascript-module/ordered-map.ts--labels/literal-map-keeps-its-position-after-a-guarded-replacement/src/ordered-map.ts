export function labels(replacement: string | null): ReadonlyMap<string, string> {
  return new Map<string, string>([
    ['a', 'old'],
    ['b', 'only'],
    ...(replacement === null ? [] : [['a', replacement] as const]),
  ]);
}
