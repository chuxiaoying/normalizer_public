# Java：聚合投影里的 CASE 决定 LET

## 设计意图

检验同一纯聚合值重复使用时的 LET：整个重复值自身含 CASE，且可在同一安全域内共享；逐项投影和其来源不变。

## 输入原始代码

```java
class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int offset = enabled ? step : 0;
        int result = 0;
        for (int item : values) {
            result = result + item + offset;
        }
        return initial + result * result;
    }
}
```

## AI 预处理结果

预处理把未知次数的循环写成求和，保留初值、原数组、每项贡献及外围平方。`offset` 是在进入聚合前已经取得的普通 Java 整数局部值。

```diff
--- a/src/Reduce.java
+++ b/src/Reduce.java
@@ -1,10 +1,10 @@
 class Reduce {
     static int run(int[] values, boolean enabled, int initial, int step) {
         int offset = enabled ? step : 0;
         int result = 0;
-        for (int item : values) {
-            result = result + item + offset;
-        }
+        /* @normalizer-ir
+           result = sum from values as value yield value + offset
+        */
         return initial + result * result;
     }
 }
```

## 归一化最终结果

每项贡献与之前的累计值无关。开关为假，输入 [3, 4] 得总数 7，平方后加上 initial = 1 得 50。聚合结果被使用两次，且其投影包含 CASE，提取后少一份 CASE，且没有跨守卫或移动外部观察，符合《统一定形》的 LET 规则。

```yaml
schema: normalizer.pers/1
entry: { function: src/Reduce.java#Reduce.run, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    enabled: { position: 1, type: bool }
    initial: { position: 2, type: number }
    step: { position: 3, type: number }
R: |-
  LET $l_0 = COALESCE(SUM(SELECT value + CASE WHEN enabled THEN step ELSE 0 END FROM values), 0)
  IN initial + $l_0 * $l_0
```

| 对照 | 预期 |
| --- | --- |
| 反写同一个 CASE，改局部名字 | 完整 PERS 不变。 |
| 去掉条件，直接加 step | 投影是 `value + step`。已无 CASE，平方两边内联，不提 LET。 |
| 只返回一次求和结果 | R 直接写一次求和表达式，不提 LET。 |
