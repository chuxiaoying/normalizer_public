import * as Official from "@normalizer/stdlib";

export function normalized(raw: string | null, fallback: string): string {
  return Official.defaultIfBlank(Official.trim(raw), fallback);
}
