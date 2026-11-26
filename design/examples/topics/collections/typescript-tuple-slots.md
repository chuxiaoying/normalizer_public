# Hello World：回执的可选槽与尾部追踪码

## 设计意图

区分同一元组的固定槽、可选槽和 rest。省略和显式 undefined 不能把后续值移到前槽；布尔字段缺失、false、true 必须按声明与真实轨迹区分。

[本主题](README.md)

## 输入原始代码

SDK 适配器在本模块内组装回执。第一槽是必占位置的错误信息，第二槽是可选回执，剩余位置按顺序存追踪码。入口统计追踪码，仅在回执成功时计数。

```typescript
type Reply = { ok: boolean }
type Receipt = [error: string | undefined, result?: Reply, ...trace: string[]]

function receipt(failed: boolean, detailed: boolean): Receipt {
    if (failed) return ["offline"]
    if (detailed) return [undefined, { ok: true }, "sent", "ack"]
    return [undefined, { ok: true }]
}

export function traceCount(failed: boolean, detailed: boolean): number {
    const [error, result, ...trace] = receipt(failed, detailed)
    return error === undefined && result?.ok ? trace.length : 0
}
```

## AI 预处理结果

**原样通过。** 保留元组类型、数组字面量、解构及可选链。receipt 的源码完整，工程可直接分析每个槽；error 的值可以是 undefined，成功回执仍位于索引 1。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry: { function: src/receipt.ts#traceCount, returns: number }
P:
  parameters:
    failed: { position: 0, type: bool }
    detailed: { position: 1, type: bool }
R: |-
  CASE
    WHEN detailed AND NOT (failed) THEN 2
    ELSE 0
  END
```

**判别例**　成功且详细时索引 0 仍占位，追踪项恰好两项；普通成功为零；失败时缺少第二槽，即使 detailed 为真也返回零。
