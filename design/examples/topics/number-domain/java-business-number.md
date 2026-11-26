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


## 源码入口区分业务整数和原生浮点

整数进入精确业务数值域之前，要按源语言识别字面量。十六进制里的 `e`、`f` 或 `d` 可以是整数数字，例如 `0x1e3` 是 483、`0x1fd` 是 509，不能把它们当成指数或浮点后缀。

[Java 的浮点词法](https://docs.oracle.com/javase/specs/jls/se25/html/jls-3.html#jls-3.10.2)允许十进制小数点在开头或结尾，允许带符号的指数，也允许只用类型后缀；十六进制浮点必须有 `p` 或 `P` 指数。对照把这些合法拼法放进返回 `double` 的同文件辅助方法；入口即使没有调用该方法，也须报 `JAVA-IEEE-LITERAL`，提示显式预处理，不执行、不生成 PERS。AI 不得把近似值擅自换成精确小数。

指数缺数字、下划线脱离数字两端、后缀混写、十六进制小数缺指数属于非法词法，不用「需要预处理」掩盖成实际可计算的输入。同样在入口拒绝；对有限值的源码类型规则不等于实现 IEEE 计算。
