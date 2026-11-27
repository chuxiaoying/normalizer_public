# Java：布尔开关与精确等式共同筛选

## 设计意图

筛选同时使用入口布尔开关与逐行的精确整数等式。等式化简只移动字面量偏移，不改变当前行身份、重复次数和空集合行为。

## 输入原始代码

```java
class Filter {
    static int run(int[] values, int target, boolean enabled) {
        int result = 0;
        for (int item : values) {
            if (enabled && item + 2 == target) {
                result++;
            }
        }
        return result;
    }
}
```

## AI 预处理结果

把逐行筛选与计数交给关系指令；保留入口开关、相同数组和原来每行的等式。

```diff
--- a/src/Filter.java
+++ b/src/Filter.java
@@ -1,11 +1,9 @@
 class Filter {
     static int run(int[] values, int target, boolean enabled) {
-        int result = 0;
-        for (int item : values) {
-            if (enabled && item + 2 == target) {
-                result++;
-            }
-        }
+        int result;
+        /* @normalizer-ir
+           result = count from values as item where enabled == true and item + 2 == target
+        */
         return result;
     }
 }
```

## 归一化最终结果

每行仅在 `enabled` 为真且 `value + 2 == target` 时计入。精确整数中移常量得 `value == target - 2`，仍是当前行与入口值的比较。比如 `values = [3, 3, 4]`、`target = 5` 时，开关为真得到 2；为假得到 0。输入为空也是 0；不做去重。

```yaml
schema: normalizer.pers/1
entry: { function: src/Filter.java#Filter.run, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    target: { position: 1, type: number }
    enabled: { position: 2, type: bool }
R: COUNT(value FROM values WHERE enabled AND value == target - 2)
```

| 对照 | 预期 |
| --- | --- |
| 移项并调换两个条件的源码顺序 | 完整 PERS 不变。 |
| 把偏移 2 改为 3 | 筛选改为 value == target - 3。 |
| 开关取反 | 改用 NOT (enabled)，不删行内等式。 |
| 同一开关同时要求真和假 | 不可能选中任一行，结果为 0。 |
