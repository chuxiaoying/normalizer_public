# Java：有序递推体的 CASE 不越过定义边界

## 设计意图

检验同一纯有序递推值重复使用时的 LET：整个重复值自身含 CASE，且可在同一安全域内共享；递推的输入顺序和更新体不变。

## 输入原始代码

```java
class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int offset = enabled ? step : 0;
        int result = initial;
        for (int item : values) {
            result = result * 2 + item + offset;
        }
        return result * result;
    }
}
```

## AI 预处理结果

预处理只把未知次数的有序循环写成 reduce，保留初值、原数组次序、递推更新及外围平方。`offset` 是在进入递推前已经取得的普通 Java 整数局部值。

```diff
--- a/src/Reduce.java
+++ b/src/Reduce.java
@@ -1,10 +1,10 @@
 class Reduce {
     static int run(int[] values, boolean enabled, int initial, int step) {
         int offset = enabled ? step : 0;
-        int result = initial;
-        for (int item : values) {
-            result = result * 2 + item + offset;
-        }
+        int result;
+        /* @normalizer-ir
+           result = reduce values from initial as (acc, item) yield acc * 2 + item + offset
+        */
         return result * result;
     }
 }
```

## 归一化最终结果

更新依赖上一步结果，不能把递推换成可交换 SUM。初值为 1、开关为假时，输入 [3, 4] 得终值 14，交换为 [4, 3] 则得 15。平方重复使用同一个纯递推值。递推表达式自身含 CASE，提取后少一份 CASE，且没有跨守卫或移动外部观察，符合《统一定形》的 LET 规则。

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
  LET $l_0 = REDUCE(values FROM initial AS (acc, item)
                    YIELD item + acc * 2 + CASE WHEN enabled THEN step ELSE 0 END)
  IN $l_0 * $l_0
```

| 对照 | 预期 |
| --- | --- |
| 反写同一个 CASE，改局部名字 | 完整 PERS 不变。 |
| 去掉条件，直接加 step | 递推更新为 `item + step + acc * 2`：两个直接引用先于乘法，`local` 的 `item` 在 `parameter` 的 `step` 前。已无 CASE，平方两边内联，不提 LET。 |
| 只返回一次递推结果 | R 直接写一次递推表达式，不提 LET。 |
