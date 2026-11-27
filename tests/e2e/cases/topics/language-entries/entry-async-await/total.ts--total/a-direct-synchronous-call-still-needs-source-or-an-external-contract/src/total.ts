export function total(source: { read: () => number }): number {
    const value = source.read();
    return value + 1;
}
