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
```

`R` 仅在正常完成路径成立：检查命中时，返回收窄为文本的原值。非文本路径保留 `TypeError` 和原消息，不产生正常返回。不访问环境、不产生外部动作。若删掉检查并直接从这个声明返回 `string` 的入口返回 `value`，应在 TypeScript 类型检查时报错；原因是返回类型不匹配，而不是参数声明了 `unknown`。

**宽类型交付对照。** 若在同一模块加入 `type Payload = unknown`，只把形参的注解换成 `Payload`，原检查、返回和 `TypeError` 均未改变，完整 PERS 相同。把注解换为 `any` 则在入口静态拒绝，不生成 PERS；若把原检查换成 `return value as string`，断言本身不执行校验，不能保留原结果与异常，因而不是等价预处理。

**不解释宽类型值。** 若入口的返回声明也为 `unknown`，直接返回入参不需要收窄。若入口原样抛出它，异常载荷仍是同一入参，不制造异常类型或包装对象；失败路径不再产生正常返回。

**先查类型，再按模式返回。** [第四章的 `handleOrder`](../../../chapters/04-AI救赎.md#ai-04-强类型准入)原样交付。先验证 `value` 是文本，再用严格相等区分 `mode`。R 只覆盖正常完成域：`mode` 是 `"delivery"` 或 `"pickup"`，任一个等式都能区分两种前缀。按[完整二分 CASE 的结构键](../../../chapters/08-统一定形.md#cf-04-完整二分-case)，同成本时 `"delivery"` 在前，R 写成 `CASE WHEN mode == "delivery" THEN "送达：" + value ELSE "门店：" + value END`。互换源码中两个互斥分支不改变完整 PERS。不合类型的 `value` 与不支持的 `mode` 各保留原来的 `Error` 消息；只有文本检查通过后，第二个错误才可能发生。

严格比较 `value === "ready"` 也给出该路径上的实际文本值：如果此时返回原来的 `value`，R 就是 `"ready"`。其余路径照常走原来的失败分支，不能因为声明是 `unknown` 而对入参做字符串转换。
