# Hello World：重复的 CASE 表达式何时必须提取 LET

## 设计意图

对照重复 CASE、仅出现一次、重复但不含 CASE 和条件才取得的值。先完成数学化简。被绑定的子式含 CASE 并在同一合法取得域内实际重复，且提取严格降低复杂度时，必须提取 LET；否则必须保持内联。

## 输入原始代码

`adjusted` 是纯算术 `value + 1`，在外层 CASE 的条件和 ELSE 中重复使用。但被重复的表达式本身不含 CASE WHEN，不能提取 LET。派生用例对照仅用一次的 CASE。

```java
class CaseReuse {
    static int cap(int value, int limit, int base) {
        int adjusted = value + 1;
        return adjusted > limit ? limit : adjusted + base;
    }
}
```

## AI 预处理结果

**原样通过。** 这是合法的原生 Java。AI 不提前展开 `adjusted`，也不把 CASE 改写成共享扩展语法。局部重复的提取属于统一 PERS 规范化职责。

**预处理要点**：是否必须提取 LET 由统一 PERS 规则决定，不由宿主语言预处理决定。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/CaseReuse.java#CaseReuse.cap
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    limit:
      position: 1
      type: number
    base:
      position: 2
      type: number

R: |-
  CASE
    WHEN limit < value + 1 THEN limit
    ELSE base + value + 1
  END
```

**PERS 验收要点**：外层是 CASE 不等于被绑定值含 CASE。只有同一纯值本身含 CASE WHEN 且实际有两处或更多引用时，才进入 LET 候选；在同一取得域内严格降低复杂度才必须提取，不得改变观察、异常或惰性。

## 被重复的聚合本身包含完整 CASE

```java
class CaseReuse {
    static int square(int[] values, boolean enabled, int extra) {
        int offset = enabled ? extra : 0;
        int total = 0;
        for (int value : values) {
            total += value + offset;
        }
        return total * total;
    }
}
```

源码按数组顺序累加。数组长度来自输入，预处理把这段可完整表达的累加交给局部关系指令，保留初值、行表达式和后续的平方，不能预先计算总和。

```diff
--- a/src/CaseReuse.java
+++ b/src/CaseReuse.java
@@ -1,10 +1,10 @@
 class CaseReuse {
     static int square(int[] values, boolean enabled, int extra) {
         int offset = enabled ? extra : 0;
-        int total = 0;
-        for (int value : values) {
-            total += value + offset;
-        }
+        int total;
+        /* @normalizer-ir
+           total = sum from values as value yield value + offset
+        */
         return total * total;
     }
 }
```

相同聚合是乘法的两个已取得操作数，查询只依赖入口数组、行绑定和已取得的标量参数。CASE 在被绑定的表达式里面而不在外层保护域。在这个 R 内提取能减少重复结构，也不改变取得范围，因此必须提取一次。若只返回一次总和，或者偏移改为不含 CASE 的普通入参，则不能提取。

```yaml
schema: normalizer.pers/1
entry: { function: src/CaseReuse.java#CaseReuse.square, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    enabled: { position: 1, type: bool }
    extra: { position: 2, type: number }
R: |-
  LET $l_0 = COALESCE(SUM(SELECT value + CASE WHEN enabled THEN extra ELSE 0 END FROM values), 0)
  IN $l_0 * $l_0
```

若两次引用只出现在外层返回 guard 的某一臂，不能为提取 LET 把计算移到 guard 之外。

即使同一个聚合已有两处直接引用，只要第三处在 CASE 的条件域内，全局替换仍会跨越这个域。共享绑定只能替换同一安全域里的引用。受保护的第三处不能纳入无条件共享。前两处是否提取，仍要独立核对取得域和复杂度；本例不把整组内联规定为唯一代表。

## 两个独立绑定比较完整 LET，不改减法方向

两个数组分别求和，返回 `zzz` 的总和平方式减去 `aaa` 的总和平方式。两次聚合的行运算相同，但数组来源不同，不能合并成一次。

```java
class CaseReuse {
    static int difference(int[] aaa, int[] zzz, boolean enabled, int extra) {
        int offset = enabled ? extra : 0;
        int first = 0;
        for (int value : aaa) {
            first += value + offset;
        }
        int second = 0;
        for (int value : zzz) {
            second += value + offset;
        }
        return second * second - first * first;
    }
}
```

预处理只把两段累加各自写成等价的关系指令，保留结果相减的方向。

```diff
--- a/src/CaseReuse.java
+++ b/src/CaseReuse.java
@@ -1,14 +1,14 @@
 class CaseReuse {
     static int difference(int[] aaa, int[] zzz, boolean enabled, int extra) {
         int offset = enabled ? extra : 0;
-        int first = 0;
-        for (int value : aaa) {
-            first += value + offset;
-        }
-        int second = 0;
-        for (int value : zzz) {
-            second += value + offset;
-        }
+        int first;
+        /* @normalizer-ir
+           first = sum from aaa as value yield value + offset
+        */
+        int second;
+        /* @normalizer-ir
+           second = sum from zzz as value yield value + offset
+        */
         return second * second - first * first;
     }
 }
```

两份聚合各自出现两次，内部都含 CASE；它们都只依赖已经取得的入口值。两个 CASE 都在各自的行表达式里，不能把行内选择当成减法或乘法的直接参数向外提取。数学化简没有把两份不同来源的值消成一个，因此两份都要提取。

两份定义互不依赖，两个次序都可作为候选；减法的左右含义不变。

```yaml
schema: normalizer.pers/1
entry: { function: src/CaseReuse.java#CaseReuse.difference, returns: number }
P:
  parameters:
    aaa: { position: 0, type: 'number[]' }
    zzz: { position: 1, type: 'number[]' }
    enabled: { position: 2, type: bool }
    extra: { position: 3, type: number }
R: |-
  LET $l_0 = COALESCE(SUM(SELECT value + CASE WHEN enabled THEN extra ELSE 0 END FROM aaa), 0)
  LET $l_1 = COALESCE(SUM(SELECT value + CASE WHEN enabled THEN extra ELSE 0 END FROM zzz), 0)
  IN $l_1 * $l_1 - $l_0 * $l_0
```

按 [CF-09](../../../chapters/08-统一定形.md#cf-09-let-的整值比较)从同一个内联值比较两个合法定义次序，先依次绑定局部槽并重写引用，然后比较整值。两个定义的结构只在输入数组引用的身份处不同，`aaa` 的定义在前。确定整值后才派生显示名。调换两个独立求和声明的源码次序，不改变这份 PERS；交换减法两端时，定义同序，但正文必须交换引用。
