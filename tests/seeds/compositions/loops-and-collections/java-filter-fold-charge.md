# 筛选原读数，再逐项扣除

## 设计意图

循环先用原读数判断是否纳入，然后对每个命中项扣一次抵扣额。筛选和折叠共用同一序列，但作用不同；固定的抵扣额不能只从折叠结果扣一次，也不能先变换再用变换后的值筛选。数组顺序不改变这里的精确加法总和，重复的读数仍是两次独立贡献。对照题材不同于只计数的[集合筛选](../../topics/collections/java-filter.md)与从业务表读取账单的[关系聚合](../../topics/loops-and-termination/java-loop-fold.md)：这里只观察已给定数组，允许入选读数的实际贡献为负，避免把筛选条件错当成对投影的约束。

## 输入原始代码

```java
class QuotaFold {
    static int charge(int[] readings, int cutoff, int deductible) {
        int total = 0;
        for (int reading : readings) {
            if (reading > cutoff) {
                total += reading - deductible;
            }
        }
        return total;
    }
}
```

入口的数组不为 null，允许空数组；阈值、读数及抵扣额都为有符号业务整数，不额外假设投影结果非负。

## AI 预处理结果

在原循环位置交付查询和累加关系。保留原输入、判定阈值和逐项扣除，不先计算行数或结果，也不新增环境读取。

```diff
--- a/src/QuotaFold.java
+++ b/src/QuotaFold.java
@@ -1,11 +1,11 @@
 class QuotaFold {
     static int charge(int[] readings, int cutoff, int deductible) {
-        int total = 0;
-        for (int reading : readings) {
-            if (reading > cutoff) {
-                total += reading - deductible;
-            }
-        }
+        int total;
+        /* @normalizer-ir
+           total = sum from readings as reading
+               where reading > cutoff
+               yield reading - deductible
+        */
         return total;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/QuotaFold.java#QuotaFold.charge, returns: number}
P:
  parameters:
    readings: {position: 0, type: 'number[]'}
    cutoff: {position: 1, type: number}
    deductible: {position: 2, type: number}
R: COALESCE(SUM(SELECT value - deductible FROM readings WHERE value > cutoff), 0)
```

独立推导：只取索引满足 `readings[i] > cutoff` 的原数组元素，每个索引贡献一次 `readings[i] - deductible`，空集合加和为 0。没有状态、外部调用或业务异常。以 `readings = [4, 5, 5, -2]`、`cutoff = 4`、`deductible = 7` 为例，结果是 `-4`；因为两个 5 分别产生 -2，4 恰在边界所以不纳入。若改为包含边界，4 再贡献 -3，结果是 -7；若只将逐项扣除改加，入选不变，两项各贡献 12，结果是 24。空数组、无项命中都为 0；不能在空集上凭空扣一次抵扣额。
