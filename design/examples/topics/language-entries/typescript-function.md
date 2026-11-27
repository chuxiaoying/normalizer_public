# TypeScript：有运行时检查也不接受 unknown

## 设计意图

验收分析副本的强类型边界。能通过 TypeScript strict 检查，不代表能交给 Normalizer。检查分支只在执行中提供局部类型证据，不能使带 `unknown` 的交付合同通过准入。依据见 [AI救赎](../../../chapters/04-AI救赎.md#只支持强类型)。

## 输入原始代码

```typescript
export function read(value: unknown): string {
    if (typeof value !== "string") throw new TypeError("Expected text");
    return value;
}
```

## AI 预处理结果

**不能原样通过。** 本例观察的入口包含检查和失败。没有证据表明调用者已完成校验，不能改成 `value: string`，也不能插入断言、转换或新入参。如果误将原件交给 Normalizer，在第一处 `unknown` 拒绝。

## 归一化最终结果

`Panic: not-support`，诊断指向源码中的 `unknown`，无 PERS。检查写成肯定分支、调换 `typeof` 比较两端、改变失败消息、或通过断言和转换使用该值，都不改变准入结论。把它写进类型别名、对象字段或局部变量，也不能绕过准入；例如辅助模块的 catch 变量由 TypeScript 推断为 unknown 时，同样在类型预检拒绝。
