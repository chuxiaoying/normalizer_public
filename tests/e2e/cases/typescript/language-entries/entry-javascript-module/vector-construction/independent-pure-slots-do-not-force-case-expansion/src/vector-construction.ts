export function pair(enabled: boolean, left: number, right: number, fixed: number, gate0: boolean, gate1: boolean, gate2: boolean, gate3: boolean, gate4: boolean, gate5: boolean, gate6: boolean, gate7: boolean): readonly number[] {
  return [gate0 ? 1 : 0, gate1 ? 1 : 0, gate2 ? 1 : 0, gate3 ? 1 : 0, gate4 ? 1 : 0, gate5 ? 1 : 0, gate6 ? 1 : 0, gate7 ? 1 : 0];
}
