# 内部成员选择：相同候选不合并两次选择

## 设计意图

检验返回表达式保留实际候选、已取得的缺省及选择依赖。两次独立选择不合并，同一次记录的不同字段不拆成两次；有依赖的顺序优先于展示排序。

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
R: |-
  CHOOSE(values DEFAULT backup AS member0 YIELD
    CHOOSE(values DEFAULT backup AS member1 YIELD member0 + member1))
```

两次真实选择不是两个外部调用，不生成 S，也不能因为候选和缺省相同而合并。候选为 [2, 7] 时，4、9、14 都是合法返回；空数组结果为 backup + backup。交换相加的引用不变，改成减法时两处选择还是原本次序，而减法的业务参数次序不可交换。此基准不声明通用对象、任意条件或与循环／外部调用的组合。

### 已知候选先执行常量语义

空的本地数组直接得到缺省值，单元素或所有元素相等的本地数组直接得到该元素，直接化简返回表达式。长度已知的零填充分配无需实际复制出每个零。已知但含不同值的数组仍有多种合法成员：例如 `[2, 7]` 可作为两次独立选择的候选，不能把“常量数组”误写成“只选择某一个常量”。数组元素先按原顺序求值；之后选择合同只观察可能值的集合而无概率承诺。因此已知候选 `[7, 2, 7]` 和 `[2, 7]` 采用同一升序去重支持集，仍然保留两次选择。

## 空候选使用已取得的入口 E 字段

```java
import normalizer.intrinsics.N;
class ChoiceBase { int value; }
class StatePick {
    static final StatePick INSTANCE = new StatePick();

    @N.EntryState ChoiceBase seed = new ChoiceBase();
    int choose(int[] values, int seed) {
        int fallback = this.seed.value;
        int selected;
        /* @normalizer-ir selected = choose from values else fallback */
        return selected;
    }
}
```

**原样通过。** 状态字段是已取得的整数，入口参数虽也叫 seed，但不是缺省值。R 的缺省值必须保留 E 的真实取得路径；不生成调用或额外 P。

```yaml
schema: normalizer.pers/1
entry: { function: src/StatePick.java#StatePick.INSTANCE.choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    seed: { position: 1, type: number }
E:
  seed: { kind: state, source: src/StatePick.java#StatePick.INSTANCE.seed, fields: { value: number } }
R: CHOOSE(values DEFAULT E.seed.value AS member0 YIELD member0)
```

## 空候选复用已经发生的外部整数回执

```java
import normalizer.intrinsics.N;

class ReceiptPick {
    static final ReceiptPick INSTANCE = new ReceiptPick(new Checkpoints());

    @N.External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = checkpoints.read(code);
        int second = checkpoints.read(first);
        int selected;
        /* @normalizer-ir selected = choose from values else second */
        return selected;
    }
}

class Checkpoints { native int read(int code); }
```

**原样通过。** 第一个回执作为第二次真实调用的参数，两次调用都保留。空数组只使用第二个回执作为缺省值，不重发请求，不把候选选择计为外部调用。此处两个端口结果都是无异常、无条件取得的已声明整数；条件调用不由本例自动放行。

```yaml
schema: normalizer.pers/1
entry: { function: src/ReceiptPick.java#ReceiptPick.INSTANCE.choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    code: { position: 1, type: number }
S:
  - call: { external: src/ReceiptPick.java#ReceiptPick.INSTANCE.checkpoints.read, args: [code], returns: number, result: '$e_0' }
  - call: { external: src/ReceiptPick.java#ReceiptPick.INSTANCE.checkpoints.read, args: ['$e_0'], returns: number, result: '$e_1' }
R: CHOOSE(values DEFAULT $e_1 AS member0 YIELD member0)
```


结果公式中严格整数加法可按结构键交换；源码的 `selected + second` 与反写都在该次 `CHOOSE` 的 `YIELD` 内写作 `member0 + $e_1`；两者均为直接数值引用，`local` 身份标签在 `receipt` 前。减法保留源码左右。若同一调用链上有 11 个同合同的回执，在稳定身份一级按事件序位置比较，因此位置 2 在 10 前；`$e_2`、`$e_10` 只是显示名。公式排序不重排 S 的调用链。


### 只在入口条件成立时发生的选择

如果 `backup == 0` 时执行第一次选择并直接返回，那么第二处选择只在 `backup != 0` 时发生。若两臂选择的候选或缺省不同，R 在各自的 CASE 臂写对应选择，不跨条件绑定；若两臂的可能返回完全相同且选择没有额外观察，则可归约为这个共同结果，而不是保留一个空的业务区别。若守卫本身取决于已选成员，保留它对同一次选择的依赖，不独立重选，也不把成员当成全域常量；能在同一 `YIELD` 内表达时保留其词法绑定，超出已定义的表达边界时报告缺口。

## 选择结果作为有序递推的初值

```java
class MixedTemporal {
    static int settle(int[] values, int backup) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int total;
        /* @normalizer-ir total = sum from values as item yield item + picked */
        return total + picked;
    }
}
```

**原样通过。** 任选一次，所得成员既是每项的增量，也是加法的一个操作数。聚合写在选择的 `YIELD` 里，两处引用都是同一成员。改变纯加法的书写方向不改变这项依赖。

```yaml
schema: normalizer.pers/1
entry: { function: src/MixedTemporal.java#MixedTemporal.settle, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    backup: { position: 1, type: number }
R: CHOOSE(values DEFAULT backup AS member0 YIELD member0 + COALESCE(SUM(SELECT value + member0 FROM values), 0))
```

若新增已取得的布尔参数 `enabled` 并仅在 `!enabled` 时选择，则 R 写 `CASE WHEN enabled THEN backup ELSE CHOOSE(values DEFAULT backup AS member0 YIELD member0) END`。真臂不选，假臂才选；空数组仍取备用值。

**已取得的聚合缺省。** 候选为空时可取已经计算的闭合整数聚合。对输入数组先筛正数再求和，再供 `choose` 使用；也可在区间 `[1, hi)` 内筛出大于 `threshold` 的位置并求平方和。只有候选为空才采用缺省；不能因为聚合带过滤或来自整数区间就把已取得的值拒掉。

已取得的带筛选去重计数或有序首项也能作为缺省，保留缺省值外层的绑定；候选非空时仍采用被选中的实际成员。

**数据表聚合缺省。** 将已取得的表快照按金额求和，再作为任选的缺省，同样保留原来的 E。若只在 `enabled` 为真时读表，PERS 的取得记录也必须受其保护；不能把它提到入口或把缺省换成另一份表读。
分组或连接得到的闭合组数、匹配数也沿同一条规则，不丢分组键、连接条件或任一已取得的表。

缺省也可以使用已经取得的有序数组位置结果。先以 `first from data[1:] else 0` 取第二项，再将该结果用作 `choose` 的缺省值。若 `data` 少于两项，所取得的值是 0；不能因为候选为空而在无第二项时执行一次越界读取。
