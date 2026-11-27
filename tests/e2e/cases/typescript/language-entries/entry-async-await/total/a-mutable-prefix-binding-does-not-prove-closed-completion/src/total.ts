export async function total(source: { read: () => Promise<number> }): Promise<number> {
    let value = await source.read();
    return value + 1;
}
