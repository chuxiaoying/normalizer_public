export async function total(ports: { read: () => Promise<number> }): Promise<number> {
    const value = await ports.read();
    return value + 1;
}
