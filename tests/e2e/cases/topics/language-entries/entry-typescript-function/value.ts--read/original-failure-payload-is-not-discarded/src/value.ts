export function read(value: unknown): string {
    if (typeof value !== "string") throw new TypeError("Text required");
    return value;
}
