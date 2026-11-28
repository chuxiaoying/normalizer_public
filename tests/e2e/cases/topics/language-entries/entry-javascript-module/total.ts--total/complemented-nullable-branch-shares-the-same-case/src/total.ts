export function total(enabled: boolean): string | null {
  return !enabled ? null : "ready";
}
