import type { Payload } from "./payload.js";
export function run(value: Payload): void {
  throw value;
}
