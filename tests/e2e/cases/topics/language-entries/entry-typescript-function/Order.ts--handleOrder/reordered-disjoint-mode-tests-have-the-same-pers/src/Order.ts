export function handleOrder(mode: unknown, value: unknown): string {
    if (typeof value !== "string") throw new Error("Expected text");
    if (mode === "delivery") return "送达：" + value;
    if (mode === "pickup") return "门店：" + value;
    throw new Error("Unknown mode");
}
