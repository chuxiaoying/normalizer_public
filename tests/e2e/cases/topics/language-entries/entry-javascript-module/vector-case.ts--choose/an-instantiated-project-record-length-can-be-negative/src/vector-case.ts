type ReadonlyArray<T> = { readonly length: T };

export function choose(value: ReadonlyArray<number>): boolean {
    return value.length < 0;
}
