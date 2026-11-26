# Hello World：带初值的 reduce

## 设计意图

检验显式初值、空集合、单值和多个值的累积。对非交换的递推保留顺序；不能默认把首项当初值，也不能因为改写成聚合而忽略初值。

## 输入原始代码

集合聚合从调用者给定的初值开始，空集合返回该初值。

```java
class ReduceValues {
    static int sumFrom(int[] values, int initial) {
        int total;
        /* @normalizer-ir
           total = reduce values from initial as (sum, value) yield sum + value
        */
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** 初值、元素顺序和累加函数都保留在局部 reduce 语义中。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/ReduceValues.java#ReduceValues.sumFrom, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    initial: { position: 1, type: number }
R: initial + COALESCE(SUM(SELECT value FROM values), 0)
```

**验收要点**：reduce 的初值是业务输入，不能在空集合或非空集合中被内核丢掉。
