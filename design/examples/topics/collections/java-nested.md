# Hello World：嵌套集合的展开与聚合

## 设计意图

检验嵌套集合中空外层、空内层和多元素的展开。每个成员按来源与实际展开次序出现；只改变可交换的最终聚合才能忽略中间排列。

## 输入原始代码

外层集合包含多个内层数列，结果是所有内层元素的总和。

```java
class NestedValues {
    static int total(int[][] groups) {
        int total;
        /* @normalizer-ir
           total = sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** 两层集合关系都保留，空内层和空外层都贡献零。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/NestedValues.java#NestedValues.total, returns: number }
P:
  parameters:
    groups: { position: 0, type: 'number[][]' }
R: COALESCE(SUM(SELECT COALESCE(SUM(SELECT value FROM grp AS value), 0) FROM groups AS grp), 0)
```

对于 `[[], [2, -3, 2], [7]]`，原例返回 8；逐元素加 1 返回 12；只保留正数返回 11。空的外层数组返回 0；空内层数组不贡献任何元素或偏移。若用 `+=` 把同一求和累加到已初始化的 7，空外层仍保留 7，上述样例得到 15；它与把总量覆盖成内层之和不同。

**验收要点**：嵌套集合的层级和空集贡献都属于业务结果，不能被预处理扁平化成一条未经证明的数组。
