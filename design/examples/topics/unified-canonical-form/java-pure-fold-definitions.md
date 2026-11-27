# 两个纯有序递推的值表达式

## 设计意图

检验有序递推保留各自输入、更新关系以及递推间的依赖。空序列保留固有初值，条件取得的初值不能在取得前使用。

## 输入原始代码

给定两个已取得的非空引用数组，元素是精确整数（数组允许长度为零）。从相同初值出发逐项更新，再求两个终值之差。

```java
class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue = initial;
        for (int leftItem : left) {
            leftValue = leftValue * 2 + leftItem;
        }
        int rightValue = initial;
        for (int rightItem : right) {
            rightValue = rightValue * 2 + rightItem;
        }
        return leftValue - rightValue;
    }
}
```

## AI 预处理结果

两个数组的长度来自输入；使用《关系代数》规定的有序递推一次描述每条循环，保留各自的初值、遍历次序与更新关系。不按样本长度展开，也不把纯定义改成外部事件。

```diff
--- a/src/CanonicalFolds.java
+++ b/src/CanonicalFolds.java
@@ -1,13 +1,13 @@
 class CanonicalFolds {
     static int difference(int[] left, int[] right, int initial) {
-        int leftValue = initial;
-        for (int leftItem : left) {
-            leftValue = leftValue * 2 + leftItem;
-        }
-        int rightValue = initial;
-        for (int rightItem : right) {
-            rightValue = rightValue * 2 + rightItem;
-        }
+        int leftValue;
+        /* @normalizer-ir
+           leftValue = reduce left from initial as (leftAcc, leftItem) yield leftAcc * 2 + leftItem
+        */
+        int rightValue;
+        /* @normalizer-ir
+           rightValue = reduce right from initial as (rightAcc, rightItem) yield rightAcc * 2 + rightItem
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
  REDUCE(left FROM initial AS (acc, item) YIELD item + acc * 2)
  - REDUCE(right FROM initial AS (acc, item) YIELD item + acc * 2)
```

两个独立纯递推分别占据减法左右，不能互换。acc 和 item 只在定义体内有效；不泄漏源码的私有变量名。初值为 1、左侧 [3,4] 时左值为 14；换为 [4,3] 则为 15，证明不能重排元素。 若改为从倍增的累加值减去当前元素，减法方向不可交换。仅观察左侧终值时，独立且未使用的右递推不发布；E 的同级字段可以各自作为独立初值，不能从字段类型猜出依赖。若左递推以右递推终值为初值，左递推的 `FROM` 槽使用右递推表达式，先取得初值再遍历左侧。

## 初值依赖已经取得的 E 状态字段

下例展示已经闭合的分析副本。状态字段已经在入口取得，无写入、RPC 或异常。读取名称仍来自 E，而不是伪装成新增参数。将这个明确的依赖保留在 R 的递推初值中。

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
           result = reduce values from initial as (acc, item) yield acc * 2 + item
        */
        return result;
    }
}
```

**原样通过。** 初值读取不改变入口顺序；输出仍使用对应的公开状态路径。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalFolds.java#CanonicalFolds.INSTANCE.fromState, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
E:
  seed: { kind: state, source: src/CanonicalFolds.java#CanonicalFolds.INSTANCE.seed, fields: { value: number } }
R: REDUCE(values FROM seed.value AS (acc, item) YIELD item + acc * 2)
```

若入口参数也叫 `seed`，初值必须写成 `E.seed.value`，不能捕获同名 P。

数组的私有局部别名不改变公开输入名。若把整个递推放进仅在某个条件成立时调用的函数，必须保持真实取得域；不能把守卫下才取得的初值变成无条件取得。

对照精确整数递推和记录值：取得域完整不改变值域，记录不能作为可交换的整数。不能仅凭类型消去尚未取得的整数值。
