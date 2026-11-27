export function pair(enabled: boolean, left: number, right: number, fixed: number): readonly number[] {
  return [enabled ? left : right, !enabled ? right : fixed];
}
