# Hello World：重复元素与稳定顺序

## 输入原始代码

统计集合中不同元素的数量。重复值只贡献一个，集合元素本身的顺序仍属于输入关系。

```java
class DistinctValues {
    static int distinctCount(int[] values) {
        int count;
        /* @normalizer-ir
           count = count distinct from values as value
        */
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** 局部 IR 明确表达去重和计数，AI 不把重复元素预先删除成某个样例常量。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/DistinctValues.java#DistinctValues.distinctCount, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: COUNT(DISTINCT value FROM values)
```

**升级思想**　去重改变元素集合，不能把输入顺序和重复关系一并抹掉后再猜测结果。

例如输入 `[3, 3, -1]`，原例返回 2；过滤非负值后再去重返回 1；不去重的对照返回 3。
