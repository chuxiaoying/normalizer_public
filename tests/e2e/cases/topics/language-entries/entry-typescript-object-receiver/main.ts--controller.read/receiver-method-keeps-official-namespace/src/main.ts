import * as N from "@normalizer/stdlib";

export const controller = {
  useDefault: true,
  read(raw: string | null, fallback: string): string {
    return this.useDefault ? N.defaultIfBlank(N.trim(raw), fallback) : fallback;
  }
} as const;
