export function pair(enabled: boolean, ready: boolean, left: number, right: number, fixed: number): readonly number[] {
  return [enabled && ready ? left + 1 : right + 2, enabled && ready ? fixed + 3 : fixed - 3];
}
