# 整数分区：有效域合并不丢首命中

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

在整数域，首支是到 -1 为止；第二和第三支的有效域分别为 0 到 2、3 到 5，按相邻整数区间合并，再按结构键排列同一 guard 的两个比较。写成显式的闭区间能得到同一 PERS。若把前两个可重叠的源码判断调换，负数将先返回 20，10 从此不可达，不能继续用上述结果。
