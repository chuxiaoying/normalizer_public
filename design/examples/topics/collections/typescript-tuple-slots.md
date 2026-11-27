# Hello World：回执的可选槽与尾部追踪码

## 设计意图

区分同一元组的固定槽、可选槽和 rest。省略和显式 undefined 不能把后续值移到前槽；布尔字段缺失、false、true 必须按声明与真实轨迹区分。

条件的变化轴是分支是否可达：辅助函数中的字面量条件，以及由调用处传入的已知布尔常量，都只选择可达分支。常真的分支已经返回时，不要求未走到的后继语句补一个返回；选中的分支正常结束而没有返回时（包括空块），则继续执行后继语句，保留其中的符号化分支。多层常量条件同样按原执行顺序选择。符号化入参不能当成常量，必须保留两条可达路径。先按 TypeScript 类型检查判断源码，再做可达性归约；类型检查报告的错误不能因为发生在不会执行的分支而消失。

本回执合同的错误槽只接受文本或 undefined，回执记录字段只携带字面布尔值。在声明保持不变时，把错误槽换成数值，或给作为 `Reply` 直接返回的对象字面量加未声明的字段，先按 TypeScript 的类型规则校验，不能因为入口只数追踪码而跳过。若连声明一起合法扩展，则按真实新合同执行，不把改动后的合同冒充原来的。

此准备形式直接调用同一模块的辅助函数，并从它的返回槽观察结果；把入口换成无调用的字面初始化不是这条调用合同。辅助条件及返回槽观察不得把无源码的 JavaScript 真值调用当作已有布尔或槽操作。

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
