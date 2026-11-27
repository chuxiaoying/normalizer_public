export function read(value: unknown): string {
    if ("string" === typeof value) {
        return value;
    }
    throw new TypeError("Expected text");
}
