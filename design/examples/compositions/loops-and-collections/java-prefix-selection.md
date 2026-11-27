# 在有序费率表的终止标记前选第一项

## 设计意图

原业务按条款优先级访问四个费率，首个严格高于本次阈值的费率胜出；零不是普通不命中项，而是立即终止扫描的标记。顺序和提前退出共同决定哪项有资格。这不同于[关系数据里的筛选后取首项](../../topics/loops-and-termination/java-loop-early-exit.md)：把零过滤掉再取首项会错误地看到原本不可达的条款。

## 输入原始代码

```java
class PrefixSelection {
    static int firstAbove(int cutoff, int fallback) {
        int[] tariff = {7, 2, 0, 9};
        for (int i = 0; i < 4; i++) {
            int value = tariff[i];
            if (value == 0) return fallback;
            if (value > cutoff) return value;
        }
        return fallback;
    }
}
```

## AI 预处理结果

此入口的数组固定四项，计数器只取 0、1、2、3。由于目前不支持一般的符号下标读取，把这一处索引明确交付成四个等义固定位置选择。只访问命中的一项，仍由原循环决定何时执行索引、何时停止；不排序数组，不提前选费率，也不把原循环展开成 PERS。

```diff
--- a/src/PrefixSelection.java
+++ b/src/PrefixSelection.java
@@ -1,11 +1,11 @@
 class PrefixSelection {
     static int firstAbove(int cutoff, int fallback) {
         int[] tariff = {7, 2, 0, 9};
         for (int i = 0; i < 4; i++) {
-            int value = tariff[i];
+            int value = i == 0 ? tariff[0] : i == 1 ? tariff[1] : i == 2 ? tariff[2] : tariff[3];
             if (value == 0) return fallback;
             if (value > cutoff) return value;
         }
         return fallback;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/PrefixSelection.java#PrefixSelection.firstAbove, returns: number}
P:
  parameters:
    cutoff: {position: 0, type: number}
    fallback: {position: 1, type: number}
R: |-
  CASE
    WHEN cutoff > 6 THEN fallback
    ELSE 7
  END
```

精确整数上的两个守卫 `cutoff < 7` 与 `cutoff > 6` 互补；《统一定形》在成本相同时选 `>`，所以默认结果写在 THEN。若阈值小于 7，第一项获选。否则 2 也不可能严格超过阈值，下一项零结束扫描，故后备值胜出。有限下标从不越界，不新增异常、环境读取或效果。阈值为 3 得 7；阈值为 7 或 8 均得后备值，即使最后一项 9 高过阈值。

把前两项调换后，阈值小于 2 时先返回 2，阈值为 2 至 6 时返回 7，其余返回后备值。去掉数组内真实的终止点后，阈值 7 或 8 能到达 9；阈值较低仍先选 7。只把零之后的 9 改成 99，不改变任何观察。不能用最小/最大值、排序或全数组筛选代替原来的逐项首命中。
