# 内部成员选择：相同候选不合并两次选择

## 输入原始代码

业务允许每次从非空整数数组独立任选一个成员；空数组两次都取调用者给定的备用值。不观察具体容器实现的选择策略、概率或顺序。

```java
import java.util.Arrays;

class CanonicalPick {
    static int total(int[] values, int backup) {
        int first = Arrays.stream(values).unordered().findAny().orElse(backup);
        int second = Arrays.stream(values).unordered().findAny().orElse(backup);
        return first + second;
    }
}
```

## AI 预处理结果

保留两次选择的位置和实际候选来源。只有此业务合同允许任选，不能把一般的 findFirst 或有顺序要求的算法改成 choose。

```diff
--- a/src/CanonicalPick.java
+++ b/src/CanonicalPick.java
@@ -1,9 +1,9 @@
-import java.util.Arrays;
-
 class CanonicalPick {
     static int total(int[] values, int backup) {
-        int first = Arrays.stream(values).unordered().findAny().orElse(backup);
-        int second = Arrays.stream(values).unordered().findAny().orElse(backup);
+        int first;
+        /* @normalizer-ir first = choose from values else backup */
+        int second;
+        /* @normalizer-ir second = choose from values else backup */
         return first + second;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalPick.java#CanonicalPick.total, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    backup: { position: 1, type: number }
T:
  - { id: $t_0, op: member_choice, when: 'true', members: values, empty: backup }
  - { id: $t_1, op: member_choice, when: 'true', members: values, empty: backup }
R: '$t_0 + $t_1'
```

两次真实选择不是两个外部调用，不生成 S，也不能因为候选和缺省相同而合并。候选为 [2, 7] 时，4、9、14 都是合法返回；空数组结果为 backup + backup。交换相加的引用不变，改成减法时两处选择还是原本次序，而减法的业务参数次序不可交换。此验收不声明通用对象选择、条件选择或与循环／外部调用的组合。
