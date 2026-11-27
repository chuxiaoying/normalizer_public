# Java：有序递推体的 CASE 不越过定义边界

## 设计意图

区别有序递推体里的 CASE 与同一个 R 值中直接重复的 CASE。递推体保留独立的有序定义；两次引用递推终值既不代表两次执行递推，也不构成 R 的 CASE 共享候选。

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

更新依赖上一步结果，不能把递推换成可交换 SUM。初值为 1、开关为假时，输入 [3, 4] 得终值 14，交换为 [4, 3] 则得 15。CASE 属于 T 的递推定义，R 中重复的是已经得到的 `$t_0`，不是 CASE；只发布一份 T，不生成 LET，也不把 T 内联到 R。

```yaml
schema: normalizer.pers/1
entry: { function: src/Reduce.java#Reduce.run, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    enabled: { position: 1, type: bool }
    initial: { position: 2, type: number }
    step: { position: 3, type: number }
T:
  - { id: $t_0, op: fold, type: number, source: values, initial: initial, accumulator: acc, element: item, body: 'item + acc * 2 + CASE WHEN enabled THEN step ELSE 0 END' }
R: $t_0 * $t_0
```

| 对照 | 预期 |
| --- | --- |
| 反写同一个 CASE，改局部名字 | 完整 PERS 不变。 |
| 去掉条件，直接加 step | T 的 body 改变；仍只发布一个 T，不增加 LET。 |
| 只返回一次递推结果 | R 改为 $t_0；T 不变。 |
