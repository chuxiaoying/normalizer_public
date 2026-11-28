import { keep } from "./helper.js";

export function run(raw: string): string {
    return keep(true ? raw : raw.trim());
}
