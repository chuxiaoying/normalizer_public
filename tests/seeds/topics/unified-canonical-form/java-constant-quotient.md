# Java 常量商和余数：先取得操作数，再执行整数运算

## 设计意图

检验合法 Java 整数常量的带符号商、余数、边界和零除数。先取到操作数，再按整数语义执行；未满足非零域不能假造 R，异常处理仍由源码确定。

## 输入原始代码

把整数商与余数放在不同位，可辨认负数是否错误地向下取整或取了非负余数。

```java
class CanonicalQuotient {
    static long signed() {
        return (-13 / 5) * 10 + (-13 % 5);
    }
}
```

**原样通过。** 商为 `-2`，余数为 `-3`，归一化由内核执行，AI 不预填答案。等价的局部变量写法得到同一个常量。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuotient.java#CanonicalQuotient.signed, returns: number }
P: { parameters: {} }
R: '-23'
```

## 超出宿主字长的结果仍是精确整数

```java
class CanonicalQuotient {
    static long large() { return -9223372036854775808L / -1L; }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuotient.java#CanonicalQuotient.large, returns: number }
P: { parameters: {} }
R: '9223372036854775808'
```

这里遵循普通业务整数的无回绕约定；不宣称是 Java `long` 机器字回绕。右操作数仍必须是合法、已取得的整数。

## 除以零不产生正常返回值

```java
class CanonicalQuotient {
    static int guarded(boolean fail) {
        if (fail) return 5 / 0;
        return 7;
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuotient.java#CanonicalQuotient.guarded, returns: number }
P: { parameters: { fail: { position: 0, type: bool } } }
R: '7'
throws:
  - { type: java.lang.ArithmeticException, when: fail }
```

真实异常只在原路径发生。`% 0` 同样抛该异常；若在入口内捕获，则执行原 handler。

```java
class CanonicalQuotient {
    static int caught() {
        try { return 7 % 0; }
        catch (ArithmeticException ignored) { return 11; }
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuotient.java#CanonicalQuotient.caught, returns: number }
P: { parameters: {} }
R: '11'
```

本例检验两端都是已知整数时的常量折叠，不把这一条件当准入限制。`input / 5` 中的商若不能折叠，仍按源程序的整数除法语义表达，不猜一个常量。商的向零截断、余数符号和零除数异常见 [JLS §15.17.2–3](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.17.2)；普通业务整数不套用其中的宿主溢出例外。


## 符号被除数仍交付向零截断的整数商

`static long signed(int input) { return input / 5; }` 先作整数除法再拓宽；除数固定非零，没有异常路径。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuotient.java#CanonicalQuotient.signed, returns: number }
P: { parameters: { input: { position: 0, type: number } } }
R: QUOTIENT(input, 5)
```

例如 `-6 → -1`，`-4 → 0`。这里没有环境读取或写入。


### 符号余数保留向零截断和零除数异常

```java
class CanonicalQuotient {
    static int remainder(int input, int divisor) {
        return input % divisor;
    }
}
```

**原样通过。** 在 `divisor != 0` 的正常完成域，余数等于被除数减去除数与向零截断商的乘积。已经取得的两个值可以在同一表达式中复用，不重新执行它们的源码。复用 [业务数值的精确合同](../../../../design/chapters/06-业务至上.md#bf-02-业务数值的精确合同)中的 `QUOTIENT`，不用新增一种 IR 原生操作。例：`-13 % 5 = -13 - 5 × (-2) = -3`，不是向下取余。

```yaml
schema: normalizer.pers/1
entry:
  function: src/CanonicalQuotient.java#CanonicalQuotient.remainder
  returns: number
P:
  parameters:
    input: {position: 0, type: number}
    divisor: {position: 1, type: number}
R: input - divisor * QUOTIENT(input, divisor)
throws:
  - {type: java.lang.ArithmeticException, when: divisor == 0}
```

把除数固定成 `5` 时，不再有异常分支，R 是 `input - 5 * QUOTIENT(input, 5)`。乘法中已取得的标量（零次运算）排在商（一次运算）前面。

在非零除数域，`0 / divisor` 和 `0 % divisor` 都化简为 0；`divisor == 0` 的异常不能跟着删掉。整数对 `-1` 取余也是 0，除数已知非零，不添加异常。

同理，在 `divisor != 0` 且乘法按精确整数执行时，`(input * divisor) / divisor` 化为 `input`，对应余数为 0。若因子实际是 `divisor + 1`，或乘法有位宽回卷合同，就不能约去。
