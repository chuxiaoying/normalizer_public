import { double } from "./helper.js";
export function run(x: number): number {
  return double(double(x));
}
