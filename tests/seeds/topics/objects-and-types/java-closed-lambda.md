# 闭合 Lambda：创建不执行，调用才执行

## 设计意图

按[符号执行](../../../../design/chapters/01-符号执行.md#se-01-按源码执行并维护真实状态)，看得见的函数体在真实调用点执行。下面只用分析工程内的单抽象方法接口。Lambda 不对外输出，不是新 PERS 类型，也不是外部端口。

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

**原样通过。** 此处是源码内标量接口的直接局部调用，不主张支持任意引用捕获或函数值逃逸。建立函数值时记录有效 final 的 `offset` 绑定，不执行加法本体。调用时 `value` 是这次传入的参数，`offset` 仍是捕获时的值，不是新的 P 或 E。把表达式体改成 `value -> { return value + offset; }` 不改变业务观察。

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

捕获局部引用时，闭包保留的是同一个堆对象的身份，不是创建时的字段快照。比如在创建 `step -> receiver.read(step)` 后，通过别名把接收对象的 `value` 写成 9，之后调用仍会读到 9。直接调用 `receiver.read(1 / zero)` 时，若 `zero` 为 0，实参先抛除零异常；只有实参正常完成，才检查接收者是否为 null。


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


### 空白局部变量也可以成为有效 final 的捕获

```java
interface Calculation { int apply(int value); }

class Quote {
    static int run(boolean active) {
        int offset;
        if (active) { offset = 2; } else { offset = 3; }
        Calculation choose = ignored -> offset;
        return choose.apply(0);
    }
}
```

每个可继续到 Lambda 的路径都给 `offset` 赋值恰好一次，所以 `active` 为真时返回 2，否则返回 3。删除 `else` 会留下未赋值路径；在一条路径赋值两次则不再有效 final。直线的先声明后赋值也成立，赋值时间不因捕获而提早。这些前提来自 [JLS §4.12.4](https://docs.oracle.com/javase/specs/jls/se25/html/jls-4.html#jls-4.12.4) 和[确定赋值规则](https://docs.oracle.com/javase/specs/jls/se25/html/jls-16.html)。本例不把一个方法内重名声明或任意控制流都当成已经证明。
