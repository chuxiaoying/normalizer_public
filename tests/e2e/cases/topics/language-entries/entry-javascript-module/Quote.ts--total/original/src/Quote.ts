type Quote = { readonly amount: number; readonly fee: number };

export function total(input: Quote): number {
    return input.fee + input.amount;
}
