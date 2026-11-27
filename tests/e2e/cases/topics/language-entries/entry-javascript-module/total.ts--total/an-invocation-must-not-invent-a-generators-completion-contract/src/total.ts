export function* total(value: number, base: number): Generator<number, number, unknown> { yield value; return value + base; }
