import { plusOne } from "./helper.js";
import { plusOne as other } from "./second.js";
export function run(value: number): number { return other(plusOne(value)); }
