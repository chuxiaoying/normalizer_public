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
- 常量路径也不能因为中间值超过宿主的 64 位或 128 位而改变结果。把本例方法体改为先绑定 `long base = 9223372036854775807L`，再返回 `base * base * base`；每个源字面量都合法，数学结果为 `(2⁶³−1)³`，PERS 的 R 应保留完整十进制整数 `784637716923335095224261902710254454442933591094742482943`。原签名仍有两个 P 参数，但结果不依赖它们。
- 业务规则和宿主实现分别承担职责，只有业务代码明确写出的边界才进入 PERS。

## 原生浮点写法直接进入业务 number

按「业务至上」执行数值：Java 源码里的 `float`、`double` 是合法的源码类型，入口要读它们；进入归一化后，只有一个对外的业务 `number`，不复刻二进制舍入。AI 不需要改写字面量。`0.1 + 0.2` 直接得到 `0.3`。这是按业务语义执行，不是声称结果与 Java 的机器浮点计算相同。

[Java 的数值词法](https://docs.oracle.com/javase/specs/jls/se25/html/jls-3.html#jls-3.10.2)决定拼法是否合法：`.25`、`1.`、带符号的十进制指数、`f/F/d/D` 后缀都可以用；十六进制浮点的 `p/P` 指数按 2 的幂解释，`0x1.fp2` 精确为 `7.75`。`0x1e3` 仍是整数 483。没有指数数字或没有有效数字、非法下划线、相互冲突的后缀，以及 Java 不允许的隐式窄化在源码入口拒绝。非零字面量若在 Java 编译期会舍入为无穷或零，也不是合法源码；这项入口检查与通过入口后不模拟 IEEE 舍入是两件事。

首批验收直接常量、正负号、混合整数与小数的加减乘比较，以及同一个业务 `number` 在方法参数和 PERS 中的展示。不定默认小数位，不用浮点近似替代精确值。非终止小数的商、原生小数余数和显式窄化尚未建立这条源码路径的合同，不能用近似或额外编译规则假装已支持。
