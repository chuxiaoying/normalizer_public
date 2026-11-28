export function read(value: unknown): string {
    if (value === "ready") return value;
    throw new TypeError("Expected ready");
}
