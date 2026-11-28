export function pair(enabled: boolean, left: number, right: number, fixed: number): readonly number[] {
  return [enabled ? left + 1 : right + 2, fixed * 2];
}
