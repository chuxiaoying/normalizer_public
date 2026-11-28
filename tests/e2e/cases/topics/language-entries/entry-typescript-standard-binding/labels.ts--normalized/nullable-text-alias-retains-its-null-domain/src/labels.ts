import * as N from "@normalizer/stdlib";
type Label = string | null;
export function normalized(value: Label): string {
  return N.defaultIfBlank(value, "fallback");
}
