import * as N from "@normalizer/stdlib";

export function normalized(raw: string | null, fallback: string): string {
  return N.defaultIfBlank(N.trim("\u00a0"), fallback);
}
