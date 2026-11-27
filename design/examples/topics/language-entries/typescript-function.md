# TypeScript：unknown 按源码类型判断收窄

## 设计意图

验收宽类型入参在运行时检查后只在成功路径收窄，并保留失败路径及其异常载荷。依据见[符号执行](../../../chapters/01-符号执行.md#se-03-宽类型值按源码语义执行)。

## 输入原始代码

```typescript
export function read(value: unknown): string {
    if (typeof value !== "string") throw new TypeError("Expected text");
    return value;
}
```

## AI 预处理结果

原样交付。这个入口包含检查和失败，不能把 `value` 改成 `string` 或把检查挪到入口之外。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/value.ts#read, returns: text }
P:
  parameters:
    value: { position: 0, type: unknown }
R: value
throws:
  - type: TypeError
    when: '"string" != typeof(value)'
    payload: '"Expected text"'
    payload_type: text
```

`R` 仅在正常完成路径成立：检查命中时，返回收窄为文本的原值。非文本路径保留 `TypeError` 和原消息，不产生正常返回。不访问环境、不产生外部动作。若删掉检查并直接从这个声明返回 `string` 的入口返回 `value`，应在 TypeScript 类型检查时报错；原因是返回类型不匹配，而不是参数声明了 `unknown`。
