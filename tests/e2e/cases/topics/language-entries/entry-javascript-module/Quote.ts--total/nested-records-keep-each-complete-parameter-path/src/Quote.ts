type Price = { readonly amount: number };
type Quote = { readonly current: Price; readonly previous: Price };

export function total(input: Quote): number {
    return input.current.amount - input.previous.amount;
}
