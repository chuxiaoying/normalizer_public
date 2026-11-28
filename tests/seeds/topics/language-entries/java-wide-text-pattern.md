# Java Object：类型判断和分支内收窄

## 设计意图

按[第一章 SE-03](../../../../design/chapters/01-符号执行.md#se-03-宽类型值按源码语义执行)执行原生 Java 类型判断。入口的 `Object` 仍是一个宽类型入参。只有判定它是 `String` 的路径才能用同一个值作为文本，判断本身既不转换值，也不产生 E 或 S。

[本主题](README.md)

## 输入原始代码

```java
class Entry {
    static String read(Object value) {
        if (value instanceof String text) return text;
        throw new IllegalArgumentException("Expected text");
    }
}
```

## AI 预处理结果

无需改写。保留原生 `instanceof`、模式变量和另一路的真实异常。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/Entry.java#Entry.read, returns: text}
P: {parameters: {value: {position: 0, type: unknown}}}
R: value
throws:
  - type: java.lang.IllegalArgumentException
    when: '"string" != typeof(value)'
    payload: '"Expected text"'
```

| 对照 | 验收要点 |
|---|---|
| 只返回类型比较；与 `Object` 比较 | 前者返回文本类型谓词；后者只排除 `null`，不能等同于文本 |
| 先将 `value` 赋给另一个 `Object` 局部，再匹配；不匹配时返回默认文本 | 仍投影原入参，按真实守卫返回默认值，不新增输入 |
| 匹配 `null`；比较字符串字面量 | `null` 不匹配，已知字符串匹配 |
| 在未匹配的 `else` 中引用模式名；将原始 `int` 直接当作 `instanceof` 操作数 | 静态拒绝，不把模式名变成全局变量，也不隐式装箱替用户修改代码 |

模式作用域和 `null` 的匹配规则见 [JLS 6.3.2.2](https://docs.oracle.com/javase/specs/jls/se21/html/jls-6.html#jls-6.3.2.2) 和 [JLS 15.20.2](https://docs.oracle.com/javase/specs/jls/se21/html/jls-15.html#jls-15.20.2)。

操作数只执行一次。若操作数是调用 `take(value)`，且该调用先把入口状态 `seen` 加一再返回原值，不论类型匹配与否，PERS 都只有一次无条件写入 `seen + 1`；文本分支也不重新调用 `take`。

取反后，模式名进入“不满足 `if` 条件但匹配成功”的分支，而不是无条件可见。若未匹配分支直接 `throw` 或 `return`，且匹配分支可以继续，后续语句也能用模式值：例如 `if (!(value instanceof String text)) throw new IllegalArgumentException("guard"); return text;`。显式 `else` 全部退出时同理；匹配分支重新赋给 `text` 后，后续读取是新值。若未匹配分支仍可能继续，后续无该模式名，静态拒绝。反过来，静态名字可见也不能凭空造出成功路径：已知 `value` 是 `null` 时只发布异常，不发布 R。
