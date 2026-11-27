export async function total(source: { read: () => Promise<number> }): Promise<number> {
    const { value } = await source.read();
    return value + 1;
}
