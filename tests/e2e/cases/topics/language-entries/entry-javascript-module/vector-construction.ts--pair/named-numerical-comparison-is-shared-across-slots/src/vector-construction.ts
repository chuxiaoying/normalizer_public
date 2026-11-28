export function pair(left: number, right: number, fixed: number, base: number): readonly number[] {
  return [left < right ? fixed : base, left < right ? fixed + 1 : base + 1];
}
