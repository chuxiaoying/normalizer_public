# Hello World：常量边界循环由内核处理

## 输入原始代码

循环边界是源码常量，工程可以直接展开并化简，不需要 AI 预处理。

```java
class SmallLoop {
    static int total() {
        int sum = 0;
        for (int i = 0; i < 3; i++) sum += i;
        return sum;
    }
}
```

## AI 预处理结果

**原样通过。** 循环次数由常量边界确定，AI 不替换循环片段。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/SmallLoop.java#SmallLoop.total
  returns: number

P:
  parameters: {}

R: '3'
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保持常量边界的 Java `for`。 | 循环是否需要预处理由次数是否可证明决定，常量循环直接交给符号化执行内核。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 仍是三次累加过程，new 化简为常量结果 `3`。 | 内核负责常量循环的展开和数学化简，AI 只处理未知次数或不可闭合的循环。 |
