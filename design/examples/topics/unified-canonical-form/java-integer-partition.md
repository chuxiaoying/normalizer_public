# 整数分区：有效域合并不丢首命中

## 设计意图

检验整数比较端点把全域切成精确分区，首命中决定各段。可合并相邻同值区间；不相邻、重叠优先级或超宿主字长端点必须保留其真实区别。

## 输入原始代码

数量是已取得的精确整数。负数归档为 10；0 到 5 归档为 20；至少 6 归档为 30。

```java
class CanonicalQuantity {
    static int band(int quantity) {
        if (quantity < 0) return 10;
        if (quantity < 3) return 20;
        if (quantity <= 5) return 20;
        return 30;
    }
}
```

## AI 预处理结果

**原样通过。** 不把中间区间或分类结果提升为入参。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuantity.java#CanonicalQuantity.band, returns: number }
P:
  parameters:
    quantity: { position: 0, type: number }
R: |-
  CASE
    WHEN quantity <= -1 THEN 10
    WHEN quantity <= 5 AND quantity >= 0 THEN 20
    ELSE 30
  END
```

在整数域，首支是到 -1 为止；第二和第三支的有效域分别为 0 到 2、3 到 5，按相邻整数区间合并，再按结构键排列同一 guard 的两个比较。写成显式的闭区间，或者在每一支把比较两端和运算符一起换边（例如 `5 >= quantity`），都得到同一 PERS。若把前两个可重叠的源码判断调换，负数将先返回 20，10 从此不可达，不能继续用上述结果。

另有两条边界验收。若三个分支分别只匹配 -2、0、2，且 -2 与 2 共用结果，必须保留两点的并集；不能填满它们之间的区间。若第一条上界为 `9223372036854775807`、第二条上界再加一，在本项目的业务精确整数域内，第二个有效域只有 `9223372036854775808`；它不回绕，也不能被 Java 的宿主字长误判为空。

对于同一个已取得的精确整数，正、负、零覆盖整个整数域。若三个早返回分别给 1、-1、0，末尾的显式抛错不可达；这与省略第三个条件、默认返回 0 的写法应得到相同的 CASE。若只覆盖正负，`quantity == 0` 仍然可达，必须保留该分支的异常，而不是把它当正常返回。若第三个条件比较另一个变量，也不能证明该并集穷尽。

### 精确小数不是相邻整数端点

派生用例改用已取得的精确小数，首支匹配至多 0，次支匹配小于 1。首命中下，`0` 返回 10、`0.25` 返回 20、`1` 返回 30。不能用整数的后继来替换开区间，也不能据此删除第二支。将两个比较同时换边需要得到同一 PERS，且不新增观察或异常。

## 零分支确实抛错的对照

```java
class CanonicalQuantity {
    static int signGuarded(int quantity) {
        if (quantity > 0) return 1;
        if (quantity < 0) return -1;
        throw new RuntimeException("zero");
    }
}
```

**原样通过。** `R` 只在正常完成域使用；在这个域，`quantity <= 0` 与 `quantity < 0` 等价，零路径由下方的异常单独记录。把正数比较写成 `0 < quantity` 不改变结果。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuantity.java#CanonicalQuantity.signGuarded, returns: number }
P: { parameters: { quantity: { position: 0, type: number } } }
R: |-
  CASE
    WHEN quantity <= 0 THEN -1
    ELSE 1
  END
S: []
throws:
  - { type: java.lang.RuntimeException, when: 'quantity == 0', payload: '"zero"' }
```

### 内核证明的补充验收

把 `x < 0`、`x == 0`、`x > 0` 视为三个独立布尔原子，不能证明它们穷尽。内核单测直接给覆盖证明器同一个精确整数上的三条 guard，要求整式归约证明穷尽；把最后一条改用 `y` 则不成立（例如 `x=1, y=0`）。源码用例已经验收正、负、零三条业务路径，但执行器可以提前消去尾路径，不能靠源代码固定到证明器的这一层。
