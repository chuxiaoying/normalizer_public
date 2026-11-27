import * as N from "@normalizer/stdlib";

export function normalized(N: string, pick: boolean): string {
  return pick ? N.trim() : "Unknown";
}
