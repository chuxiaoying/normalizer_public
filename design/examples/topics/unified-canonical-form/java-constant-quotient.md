# Java 常量商和余数：先取得操作数，再执行整数运算

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
S: []
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

本种子只接纳求值后两端都是具体整数的 `/` 和 `%`。例如 `input / 5` 仍不属于本切片，不能用猜测的商替代符号化计算。商的向零截断、余数符号和零除数异常见 [JLS §15.17.2–3](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.17.2)；普通业务整数不套用其中的宿主溢出例外。
