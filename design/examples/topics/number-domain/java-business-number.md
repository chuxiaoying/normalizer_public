# Hello World：业务 number 不受 Java 位宽限制

## 设计意图

对照宿主字长边界内外的业务整数，常量和符号都按精确值域运算。超过 int 或 long 上限不回卷；只有源码明确的位宽或回卷业务才另走其合同。

## 输入原始代码

入口把费用加到余额上。Java 方法声明为 `int`，业务规则要求结果使用 Normalizer 的无固定上限 number。调用 `balance = 2147483647`、`fee = 1` 时，业务结果仍是 2147483648。

```java
class Balance {
    static int addFee(int balance, int fee) {
        return balance + fee;
    }
}
```

## AI 预处理结果

**原样通过。** 源码没有写出回卷、溢出或截断规则。AI 不把 Java 位宽算法改写进源码，Normalizer 按业务 number 合同处理加法。

**预处理要点**

- 宿主语言的机器整数实现属于工程底层细节，业务源码没有表达时不进入归一化规则。
- 归一化关注业务逻辑，数值位宽只有在源码显式写成业务规则时才需要单独建模。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Balance.java#Balance.addFee
  returns: number

P:
  parameters:
    balance:
      position: 0
      type: number
    fee:
      position: 1
      type: number

R: balance + fee
```

`R` 描述业务加法，不携带 Java `int` 的 32 位回绕。显式业务回卷由[业务流水号回卷](java-explicit-rollover.md)这个已有种子承担。

**PERS 验收要点**

- PERS 不模拟计算机底层实现，内核使用面向业务的无限 number 域。
- 业务规则和宿主实现分别承担职责，只有业务代码明确写出的边界才进入 PERS。
