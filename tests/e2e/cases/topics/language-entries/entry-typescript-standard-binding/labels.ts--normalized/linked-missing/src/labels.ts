import * as N from "@normalizer/stdlib";
import { normalize } from "./display.js";

export function normalized(raw: string | null, fallback: string): string {
  return N.defaultIfBlank(normalize(raw), fallback);
}
