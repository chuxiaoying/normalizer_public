export function read(value: unknown): string {
    if (!value) throw new TypeError("Expected text");
    return value as string;
}
