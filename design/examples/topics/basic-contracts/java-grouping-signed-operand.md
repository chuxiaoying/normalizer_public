# Java：括号后的加减不由名称大小写决定

## 设计意图

区分 `(名称) + 值` 中分组后的二元加法与 `(long) +值` 中对带正号操作数的原生类型转换。局部变量名字大写仍是值；数组仍是引用类型，即使元素是 `long`。改变正负号、嵌套括号或改用未声明的名字时，按各自的 Java 语法和名称绑定判定，不能单凭拼写猜一个类型。

[本主题](README.md)

## 输入原始代码

```java
class SignedGrouping {
    static long read(long value) {
        long Upper = value;
        return (Upper) + 1;
    }
}
```

## AI 预处理结果

**原样通过。** `(Upper)` 读取局部变量，后面的 `+` 是二元运算。原生标量转换例如 `(long) +1` 可以接一元 `+`；引用或数组转换不能把紧跟右括号的 `+` 或 `-` 当作一元操作数的一部分。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/SignedGrouping.java#SignedGrouping.read, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: value + 1
```

| 对照 | 独立预期 |
|---|---|
| `(Upper) - 1` | 从原入参减 1；不是将负数转换成名为 `Upper` 的类型 |
| 多加一层括号，改用小写局部名，或在右括号和运算符之间加注释 | 都不改变加 1 的结果 |
| 局部名以原生类型拼写开头，后接字母、下划线或美元符号，例如 longValue、long_value、long$value | 这三个都是完整标识符。分别绑定入参、2 和 3，再加起来得到 value + 5；不截断为 long 转换 |
| `(long) +1` 和 `(long) -1` | 转换的结果分别是常量 1 和 -1；入口仍声明原有参数 |
| `(Missing) + 1` | `Missing` 是值位置中的未声明名称，入口拒绝且没有 PERS |
| `(long[]) +1` | 左侧不是可用于二元加法的分组值，也不是合法的数组转换，入口拒绝且没有 PERS |

此例不规定引用转换的求值、装箱或窄化语义，也不把合法但尚未支持的引用转换写成预期拒绝。

解析器层另验收 `(String)_token` 和 `(String)$token`：在合法 Java 声明中，操作数仍是原标识符，不能漏消费，也不能替换成空值。这只断言源码 AST，不扩大直接源码归一化对引用转换的执行合同。
