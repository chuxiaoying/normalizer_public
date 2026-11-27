type Reply = { ok: boolean }
type Receipt = [error: string | undefined, result?: Reply, ...trace: string[]]

function receipt(failed: boolean, detailed: boolean): Receipt {
    /* @normalizer-ir retained = 2 */ if (failed) return ["offline"]
    if (detailed) return [undefined, { ok: true }, "sent", "ack"]
    return [undefined, { ok: true }]
}

export function traceCount(failed: boolean, detailed: boolean): number {
    const [error, result, ...trace] = receipt(failed, detailed)
    return error === undefined && result?.ok ? trace.length : 0
}
