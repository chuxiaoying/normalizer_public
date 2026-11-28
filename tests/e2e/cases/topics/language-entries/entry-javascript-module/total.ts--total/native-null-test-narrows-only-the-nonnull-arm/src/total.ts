type Label = string | null;
export function total(value: Label): string {
    return value === null ? "fallback" : value;
}
