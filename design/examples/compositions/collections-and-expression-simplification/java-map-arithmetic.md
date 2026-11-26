# 集合映射中的算术抵消

## 设计意图

检验已取得的非空精确整数数组逐行映射。行值与入口参数在同一行内的相等偏移可以抵消；不同偏移仍保留每行的差值，空数组仍为 0。

## 输入原始代码

按原顺序累加各个数组元素。这里的整数是业务精确整数，不依附 Java 字宽回绕。

```java
class RowArithmetic {
    static int sum(int[] values, int extra) {
        int total = 0;
        for (int value : values) total += (value + extra) - extra;
        return total;
    }
}
```

## AI 预处理结果

用有序求和交付原来的局部循环，保持行作用域和未化简的算式，预处理不替内核做抵消。

```diff
--- a/src/RowArithmetic.java
+++ b/src/RowArithmetic.java
@@ -1,7 +1,9 @@
 class RowArithmetic {
     static int sum(int[] values, int extra) {
-        int total = 0;
-        for (int value : values) total += (value + extra) - extra;
+        int total;
+        /* @normalizer-ir
+           total = sum from values as value yield (value + extra) - extra
+        */
         return total;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/RowArithmetic.java#RowArithmetic.sum, returns: number}
P:
  parameters:
    values: {position: 0, type: 'number[]'}
    extra: {position: 1, type: number}
R: COALESCE(SUM(SELECT value FROM values), 0)
```

对照 `(value + 3) - 2` 使每行多一，不是整个结果多一；空数组仍为 0。证明仅适用于本例的已取得整数行，不推广到可空 SQL 字段或其他行。
