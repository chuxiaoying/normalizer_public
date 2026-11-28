import type { Quote as BillingQuote } from "./types.js";

export function run(value: BillingQuote): number {
    return value.amount;
}
