# 先按桶求和，再累计桶的影响

## 设计意图

借贷调整按桶归集。每个桶的影响是该桶净额的平方，折叠结束后才乘本次权重。外层不能把内层集合展平；即使两组输入逐项总账相同，跨桶移动条目也会改变结果。这不同于可展开的[嵌套集合加和](../../topics/collections/java-nested.md)，也不是[矩形有限循环计数](../../topics/loops-and-termination/java-nested.md)。

## 输入原始代码

```java
class BucketImpact {
    static int score(int weight) {
        int[][] buckets = {{-4, 2}, {5, 1}};
        int result = 0;
        for (int group = 0; group < buckets.length; group++) {
            int[] row = buckets[group];
            int subtotal = 0;
            for (int member = 0; member < row.length; member++) {
                subtotal += row[member];
            }
            result += subtotal * subtotal;
        }
        return result * weight;
    }
}
```

## AI 预处理结果

**原样通过。** 两层数组和下标访问是原生 Java。外层只访问两个已声明的桶，内层只访问当前桶的有效下标；边界可证明，交给符号执行按原顺序推进。AI 不拆数组、不用三元式代替下标，也不预先计算净额或平方。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/BucketImpact.java#BucketImpact.score, returns: number}
P:
  parameters:
    weight: {position: 0, type: number}
R: weight * 40
```

独立推导：第一桶净额为 -2，影响为 4；第二桶净额为 6，影响为 36，总影响为 40。精确整数权重在两层循环之后乘一次，例如权重为 3 时结果为 120。没有越界、外部读取或状态写入。

把条目改为两桶 [-4, 5]、[2, 1]，原始条目多重集和全表之和不变，但桶净额变成 1、3，总影响为 10。每桶只访问第一项时，净额变成 -4、5，总影响为 41。空桶只让自己贡献 0，不影响其余桶。例如 `[[], [5, 1]]` 的总影响仍为 36，只有所有桶的净额都为零时总影响才为零。不访问空桶的下标，也不删除入口参数声明。
