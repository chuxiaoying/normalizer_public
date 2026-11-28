import type { Domain } from "./domain.js";
import { step } from "./helper.js";
export function run(value: Domain): Domain { return step(value); }
