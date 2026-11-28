import { plusOne } from "./helper.js";

function implementation(value: number): number { return plusOne(value); }
function run(value: number): number { return value + 10; }
export { implementation as run };
