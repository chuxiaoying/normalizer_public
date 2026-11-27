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
