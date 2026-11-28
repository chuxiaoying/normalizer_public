export function read(value: unknown): string {
    if (typeof value !== "string") throw new TypeError("Expected text");
    return String(value);
}
