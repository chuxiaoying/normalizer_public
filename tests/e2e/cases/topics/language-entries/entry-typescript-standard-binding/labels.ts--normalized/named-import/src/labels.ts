import { defaultIfBlank, trim } from "@normalizer/stdlib";

export function normalized(raw: string | null, fallback: string): string {
  return defaultIfBlank(trim(raw), fallback);
}
