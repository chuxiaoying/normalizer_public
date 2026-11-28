# Hello World：逐元素映射和元素类型变化

## 设计意图

检验映射按每项真实类型执行，且结果类型允许与输入不同。空输入不运行转换，换字段或运算会改变结果，不能把映射错成过滤或聚合。

## 输入原始代码

对集合中的每个数加一，再求映射后的总和。

```java
class MapValues {
    static int sumPlusOne(int[] values) {
        int total;
        /* @normalizer-ir
           total = sum from values as value yield value + 1
        */
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** 局部 IR 只表达逐元素映射和后续聚合，保留输入集合和空集合贡献零。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/MapValues.java#MapValues.sumPlusOne, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: COALESCE(SUM(SELECT value + 1 FROM values), 0)
```

**验收要点**：map 改变的是元素表达式，不改变集合顺序和空集聚合合同。
