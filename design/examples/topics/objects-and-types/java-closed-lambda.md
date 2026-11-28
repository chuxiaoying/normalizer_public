# 闭合 Lambda：创建不执行，调用才执行

## 设计意图

按[符号执行](../../../chapters/01-符号执行.md#se-01-按源码执行并维护真实状态)，看得见的函数体在真实调用点执行。下面只用分析工程内的单抽象方法接口。Lambda 不对外输出，不是新 PERS 类型，也不是外部端口。

## 输入与预处理

```java
interface Calculation {
    int apply(int value);
}

class Quote {
    static int run(int input, int delta) {
        int offset = delta + 1;
        Calculation addOffset = value -> value + offset;
        return addOffset.apply(input);
    }
}
```

**原样通过。** 建立函数值时记录有效 final 的 `offset` 绑定，不执行加法本体。调用时 `value` 是这次传入的参数，`offset` 仍是捕获时的值，不是新的 P 或 E。把表达式体改成 `value -> { return value + offset; }` 不改变业务观察。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/Quote.java#Quote.run, returns: number}
P:
  parameters:
    input: {position: 0, type: number}
    delta: {position: 1, type: number}
R: delta + input + 1
```

## 必须保留的差异

- **不调用不执行。** 如果函数体改为 `value -> value / divisor`，并且只在条件成立时调用，那么除零只可能发生在调用路径。不能在 Lambda 建立时抛出，也不能在调用已发生时丢掉异常。
- **非法捕获在执行前拒绝。** 若 `offset` 在初始化后又被赋值，它不再是有效 final。程序在源码准入失败，不能把它按最后一个值或引用捕获继续模拟。
- Lambda 的目标类型、函数体和捕获绑定以原语言声明为准，见 [JLS §15.27](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.27)。这个种子不主张把闭包本身发布进 PERS。


## 对照：绑定方法引用在创建时检查接收者

```java
interface Reader {
    int get();
}

class Receiver {
    int read() { return 9; }
}

class Quote {
    static int bound() {
        Receiver receiver = null;
        Reader reader = receiver::read;
        return 7;
    }

    static int unusedLambda() {
        Receiver receiver = null;
        Reader reader = () -> receiver.read();
        return 7;
    }

    static int invokedLambda() {
        Receiver receiver = null;
        Reader reader = () -> receiver.read();
        return reader.get();
    }
}
```

- `bound` 创建绑定引用时立即抛出 `java.lang.NullPointerException`，不会返回 7。
- `unusedLambda` 只建立函数值，未调用函数体，正常返回 7。
- `invokedLambda` 在调用时执行函数体，才抛出同一种异常。

这三个入口都没有额外的 P、E 或 S；出现异常的入口没有正常 R。按 [方法引用的求值规则](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.13.3)和 [Lambda 的求值规则](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.27.4)，不能把两种写法当成等价，也不以分别创建的 Lambda 是否引用同一对象作为断言。
