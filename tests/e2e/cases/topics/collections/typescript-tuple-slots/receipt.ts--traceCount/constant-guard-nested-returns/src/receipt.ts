type Reply = { ok: boolean }
type Receipt = [error: string | undefined, result?: Reply, ...trace: string[]]

function receipt(failed: boolean, detailed: boolean): Receipt {
    if (!!true) {
        if (!true) return ["offline"]
        else return [undefined, { ok: true }, "sent", "ack"]
    }
    return ["offline"]
}

export function traceCount(failed: boolean, detailed: boolean): number {
    const [error, result, ...trace] = receipt(failed, detailed)
    return error === undefined && result?.ok ? trace.length : 0
}
