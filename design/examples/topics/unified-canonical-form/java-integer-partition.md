# 整数分区：有效域合并不丢首命中

## 设计意图

同一变量的上下界收紧后，要保留其他变量的单独约束和布尔条件。相同的开口上下界没有允许值；同一点的闭区间只等价于等于该点。

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
    WHEN quantity < 0 THEN 10
    WHEN quantity < 6 THEN 20
    ELSE 30
  END
```

在整数域，首支是到 -1 为止；第二和第三支的有效域分别为 0 到 2、3 到 5，按相邻整数区间合并。只有知道首支未命中才会检查第二支，那时 `quantity >= 0` 已成立，第二个守卫只保留 `quantity < 6`。写成显式的闭区间，或者在每一支把比较两端和运算符一起换边（例如 `5 >= quantity`），都得到同一 PERS。若把前两个可重叠的源码判断调换，负数将先返回 20，10 从此不可达，不能继续用上述结果。

另有两条边界验收。若三个分支分别只匹配 -2、0、2，且 -2 与 2 共用结果，必须保留两点的并集；不能填满它们之间的区间。若第一条上界为 `9223372036854775807`、第二条上界再加一，在本项目的业务精确整数域内，第二个有效域只有 `9223372036854775808`；它不回绕，也不能被 Java 的宿主字长误判为空。

对于同一个已取得的精确整数，正、负、零覆盖整个整数域。若三个早返回分别给 1、-1、0，末尾的显式抛错不可达；这与省略第三个条件、默认返回 0 的写法应得到相同的 CASE。若只覆盖正负，`quantity == 0` 仍然可达，必须保留该分支的异常，而不是把它当正常返回。若第三个条件比较另一个变量，也不能证明该并集穷尽。

显式列出负数、0 至 5、至少 6 三段时，它们也穷尽整数域。第三段后写的诊断值 99 不可达，应和不用第三个守卫、直接默认 30 的原例归到同一完整 CASE。

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

**原样通过。** `R` 只在正常完成域使用；在正常域里 `quantity > 0` 与 `quantity < 0` 互补且整式复杂度相同；按共享结构键选择 `>`。零路径由下方的异常单独记录。把正数比较写成 `0 < quantity` 不改变结果。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuantity.java#CanonicalQuantity.signGuarded, returns: number }
P: { parameters: { quantity: { position: 0, type: number } } }
R: |-
  CASE
    WHEN quantity > 0 THEN 1
    ELSE -1
  END
throws:
  - { type: java.lang.RuntimeException, when: 'quantity == 0', payload: '"zero"' }
```

### 内核证明的补充验收

把 `x < 0`、`x == 0`、`x > 0` 视为三个独立布尔原子，不能证明它们穷尽。只有它们比较同一个已经取得的精确整数，才可证明覆盖全域。把最后一条改用 `y` 即不成立，例如 `x = 1, y = 0` 时三条条件都不成立。

点值规则也要保留载荷差异：在 `x=2` 处，`2*x+3` 正好为 7，返回 7 的特例冗余，返回 8 的特例则是真实业务例外。这两个特例应给出不同的完整预期。

按 [MA-06](../../../chapters/03-数学化简.md#ma-06-用整数区间证明守卫) 先证明比较的是同一个已取得的精确整数，再求该域内的真集。不能因为值来自字段就排除它，也不能把未证为整数的小数当整数。没有整数比较对象的纯布尔条件不走这项区间证明；预算不足按 [CF-03](../../../chapters/08-统一定形.md#cf-03-化简与候选优先级) 报分析未完成，不另设区间数上限或宣称已定形。嵌套结果仍按各自有效域化简。在 `x<=0` 的首臂之后，`x<=-1` 对应的第二个不同载荷不可达，不写入分区结果。

区间比较本身也不切换到机器位宽。在精确整数域令 `limit = 9223372036854775809`。两个上界取并，`x < limit || x < limit + 7` 等于较宽的 `x < 9223372036854775816`。两个严格界限取交，`limit < x && x < limit + 2` 恰有一个点；若右端只加 1，就是空域。三条对照不把超出 Java 有符号长整数的数学端点截断或回卷。
