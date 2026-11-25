# Hello World：Java 静态函数

## 输入原始代码

这是最小的 Java 静态函数案例。函数只读取一个 `int` 参数并返回加一后的值，没有状态写入、外部边界调用、环境读取或异常路径。

```java
class HelloWorld {
    static int addOne(int value) {
        return value + 1;
    }
}
```

## AI 预处理结果

**原样通过。** 入口只读取参数并执行一次整数加法。上面的 Java 即为 Normalizer 的输入。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都将完整 Java 原样交给 Normalizer，没有本例预处理差异。 | — |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/HelloWorld.java#HelloWorld.addOne
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number

R: value + 1
```

本例只观察参数 `value` 和返回值 `value + 1`。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| `R: word_add32(value, 1)` → `R: value + 1`。 | 用无固定大小上限的业务 `number` 统一数值计算，让业务规则摆脱宿主语言的位宽与回绕。 |
| old 的最终输出只在 R 中使用 `value`；new 增加 `entry.function`、`entry.returns` 与 `P.parameters.value`，明确入口、返回类型、参数位置和类型。 | 最终产物应自行交代“正在解释哪个入口、值从哪里来”，不依赖读者从命令或源码猜测符号含义。 |
| old 输出空的 `S: []`、`T: []`、`throws: []`；new 省略这些未使用的分区。 | 结构只承载本例实际存在的信息，纯计算的结果无需被空分区包围。 |
