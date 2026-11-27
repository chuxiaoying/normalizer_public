import * as Text from "@normalizer/stdlib";

export function normalize(value: string | null): string | null {
  return Text.trim(value);
}
