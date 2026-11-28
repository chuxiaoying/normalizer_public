# 独立聚合的公开来源与取得域

## 设计意图

检验独立聚合保留各自的公开输入，并维护投影中的捕获关系。纯聚合未被使用时不发布；只在守卫下取得的值不能在取得前使用。

## 输入原始代码

给定两个已取得的非空引用数组，元素是精确整数（数组允许长度为零）。把同一个已取得的增量加到每项，再求两边的总和之差。空数组的总和为 0。

```java
class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue = 0;
        for (int leftItem : left) {
            leftValue += leftItem + initial;
        }
        int rightValue = 0;
        for (int rightItem : right) {
            rightValue += rightItem + initial;
        }
        return leftValue - rightValue;
    }
}
```

## AI 预处理结果

两个数组长度来自输入。每项只读行本身及已经取得的增量，交付各自的求和，不枚举也不另造外部事件。

```diff
--- a/src/CanonicalFolds.java
+++ b/src/CanonicalFolds.java
@@ -1,13 +1,13 @@
 class CanonicalFolds {
     static int difference(int[] left, int[] right, int initial) {
-        int leftValue = 0;
-        for (int leftItem : left) {
-            leftValue += leftItem + initial;
-        }
-        int rightValue = 0;
-        for (int rightItem : right) {
-            rightValue += rightItem + initial;
-        }
+        int leftValue;
+        /* @normalizer-ir
+           leftValue = sum from left as leftItem yield leftItem + initial
+        */
+        int rightValue;
+        /* @normalizer-ir
+           rightValue = sum from right as rightItem yield rightItem + initial
+        */
         return leftValue - rightValue;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalFolds.java#CanonicalFolds.difference, returns: number }
P:
  parameters:
    left: { position: 0, type: 'number[]' }
    right: { position: 1, type: 'number[]' }
    initial: { position: 2, type: number }
R: |-
  COALESCE(SUM(SELECT value + initial FROM left), 0)
  - COALESCE(SUM(SELECT value + initial FROM right), 0)
```

两个聚合分别占据减法左右，公开来源不能互换，行别名不泄漏。增量为 1、左侧 [3, 4] 时左值为 9；空数组为 0。仅使用左值时，独立且未使用的右聚合不发布。把右聚合用作左投影的已取得增量时，保留数据依赖，不把右侧变成一个新参数。

## 投影依赖已经取得的 E 状态字段

下例展示已经闭合的分析副本。状态字段已经在入口取得，无写入、RPC 或异常。读取名称仍来自 E，而不是伪装成新增参数。将这个明确的依赖保留在 R 的逐项投影中。

```java
import normalizer.intrinsics.N;
class Base { int value; }
class CanonicalFolds {
    static final CanonicalFolds INSTANCE = new CanonicalFolds();

    @N.EntryState Base seed = new Base();
    int fromState(int[] values) {
        int initial = seed.value;
        int result;
        /* @normalizer-ir
           result = sum from values as item yield item + initial
        */
        return result;
    }
}
```

**原样通过。** 增量读取不改变入口顺序；输出仍使用对应的公开状态路径。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalFolds.java#CanonicalFolds.INSTANCE.fromState, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
E:
  seed: { kind: state, source: src/CanonicalFolds.java#CanonicalFolds.INSTANCE.seed, fields: { value: number } }
R: COALESCE(SUM(SELECT value + seed.value FROM values), 0)
```

若入口参数也叫 `seed`，捕获的增量必须写成 `E.seed.value`，不能捕获同名 P。

数组的私有局部别名不改变公开输入名。若把整个聚合放进仅在某个条件成立时调用的函数，必须保持真实取得域；不能把守卫下才取得的增量变成无条件取得。

对照精确整数投影和记录值：取得域完整不改变值域，记录不能作为可交换的整数。不能仅凭类型消去尚未取得的整数值。


## 递推不能保证等价时拒绝

```java
class Weighted {
    static int run(int[] values, int initial) {
        int total = initial;
        for (int item : values) {
            total = total * 2 + item;
        }
        return total;
    }
}
```

从 1 出发，[3, 4] 得 14，[4, 3] 得 15。逐项不带权的求和不能保留这个差异。当前关系清单没有表达这一权重的运算，**没有等价的分析副本，不生成 PERS**。也不使用通用递推指令代替诊断。
