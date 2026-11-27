# TypeScript：对 unknown 做真实检查再返回

## 设计意图

`unknown` 是入口在收到未检查值时的类型，不是可用于业务运算的弱类型。运行到原有的 `typeof` 检查时才区分成功和失败；成功分支返回同一个字符串值，失败分支传播原错误及其消息。不把「已经是字符串」提前写成调用方保证。

## 输入原始代码

```typescript
export function read(value: unknown): string {
    if (typeof value !== "string") throw new TypeError("Expected text");
    return value;
}
```

## AI 预处理结果

**原样通过。** TypeScript 检查器证明，只有 `typeof value === "string"` 的路径走到返回语句。类型断言和 `String(value)` 都不是这条检查：前者没有运行时判断，后者可能转换值，不能替换本例。

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
    when: 'typeof(value) != "string"'
    payload: '"Expected text"'
    payload_type: text
```

`R` 只在正常完成域有定义，此时 `value` 就是原字符串。空串是有效字符串；数字、null、undefined 和 `new String(...)` 不是。错误分支不发布 R 的替代值，不制造 E、S 或额外布尔 P。

| 对照 | 预期 |
| --- | --- |
| 交换比较两端且把正常返回放进肯定分支 | 同一 PERS。 |
| 改错误消息 | 只改变异常载荷。 |
| 直接返回 unknown、用断言、以真值或转换代替检查 | 不按这个合同产生 PERS。 |
