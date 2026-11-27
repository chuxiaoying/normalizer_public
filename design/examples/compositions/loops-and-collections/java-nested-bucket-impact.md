# 先按桶求和，再累计桶的影响

## 设计意图

借贷调整按桶归集。每个桶的影响是该桶净额的平方，折叠结束后才乘本次权重。外层不能把内层集合展平；即使两组输入逐项总账相同，跨桶移动条目也会改变结果。这不同于可展开的[嵌套集合加和](../../topics/collections/java-nested.md)，也不是[矩形有限循环计数](../../topics/loops-and-termination/java-nested.md)。

## 输入原始代码

```java
class BucketImpact {
    static int score(int weight) {
        int[][] buckets = {{-4, 2}, {5, 1}};
        int result = 0;
        for (int group = 0; group < buckets.length; group++) {
            int[] row = buckets[group];
            int subtotal = 0;
            for (int member = 0; member < row.length; member++) {
                subtotal += row[member];
            }
            result += subtotal * subtotal;
        }
        return result * weight;
    }
}
```

## AI 预处理结果

业务表写死为两桶、每桶两项。证明两个边界和对应下标后，分析副本用各自的一维数组保留桶，分别把动态下标交付为对应的固定位置选择。仍由两个原生循环执行，先算每个桶的净额，再平方；不把结果预算成常量，不对任意大小或不规则二维数组推广此改写。数组是不会逃逸、没有写入的局部值，换成两个声明不会改变可观察身份或效果。

```diff
--- a/src/BucketImpact.java
+++ b/src/BucketImpact.java
@@ -1,15 +1,16 @@
 class BucketImpact {
     static int score(int weight) {
-        int[][] buckets = {{-4, 2}, {5, 1}};
+        int[] debit = {-4, 2};
+        int[] credit = {5, 1};
         int result = 0;
-        for (int group = 0; group < buckets.length; group++) {
-            int[] row = buckets[group];
+        for (int group = 0; group < 2; group++) {
+            int[] row = group == 0 ? debit : credit;
             int subtotal = 0;
-            for (int member = 0; member < row.length; member++) {
-                subtotal += row[member];
+            for (int member = 0; member < 2; member++) {
+                subtotal += member == 0 ? row[0] : row[1];
             }
             result += subtotal * subtotal;
         }
         return result * weight;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/BucketImpact.java#BucketImpact.score, returns: number}
P:
  parameters:
    weight: {position: 0, type: number}
R: weight * 40
```

独立推导：第一桶净额为 -2，影响为 4；第二桶净额为 6，影响为 36，总影响为 40。精确整数权重在两层循环之后乘一次，例如权重为 3 时结果为 120。没有越界、外部读取或状态写入。

把条目改为两桶 [-4, 5]、[2, 1]，原始条目多重集和全表之和不变，但桶净额变成 1、3，总影响为 10。每桶只访问第一项时，净额变成 -4、5，总影响为 41。空内层的局部净额仍为零，每个外层迭代贡献零，最后结果为零；不访问未发生的下标，也不能删除入口参数声明。
