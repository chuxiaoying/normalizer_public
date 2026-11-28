import type { Payload } from "./payload.js";
export function run(value: Payload): string {
  if (typeof value !== `string`) throw new TypeError(`Expected text`);
  return value;
}
