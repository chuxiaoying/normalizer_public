export function total(left: string | null, right: string | null): string {
    return left === null ? "fallback" : right;
}
