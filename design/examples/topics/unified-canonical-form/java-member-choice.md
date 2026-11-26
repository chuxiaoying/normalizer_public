# 内部成员选择：相同候选不合并两次选择

## 设计意图

检验按实际候选、已取得的缺省及依赖发布选择 T。两次独立选择不合并，同一次记录的不同字段不拆成两次；有依赖的顺序优先于展示排序。

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

两次真实选择不是两个外部调用，不生成 S，也不能因为候选和缺省相同而合并。候选为 [2, 7] 时，4、9、14 都是合法返回；空数组结果为 backup + backup。交换相加的引用不变，改成减法时两处选择还是原本次序，而减法的业务参数次序不可交换。此基准不声明通用对象、任意条件或与循环／外部调用的组合。

### 已知候选先执行常量语义

空的本地数组直接得到缺省值，单元素或所有元素相等的本地数组直接得到该元素，不生成虚构的 T。长度已知的零填充分配无需实际复制出每个零。已知但含不同值的数组仍有多种合法成员：例如 `[2, 7]` 可作为两个独立 T 的候选，不能把“常量数组”误写成“只选择某一个常量”。数组元素先按原顺序求值；之后选择合同只观察可能值的集合而无概率承诺。因此已知候选 `[7, 2, 7]` 和 `[2, 7]` 采用同一升序去重支持集，仍然保留两次选择。

## 空候选使用已取得的入口 E 字段

```java
import normalizer.annotations.EntryState;
class ChoiceBase { int value; }
class StatePick {
    @EntryState ChoiceBase seed;
    int choose(int[] values, int seed) {
        int fallback = this.seed.value;
        int selected;
        /* @normalizer-ir selected = choose from values else fallback */
        return selected;
    }
}
```

**原样通过。** 状态字段是已取得的整数，入口参数虽也叫 seed，但不是缺省值。发布 T 时必须保留 E 的真实取得路径；不生成调用或额外 P。

```yaml
schema: normalizer.pers/1
types:
  ChoiceBase: { fields: { value: number } }
entry: { function: src/StatePick.java#StatePick.choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    seed: { position: 1, type: number }
E:
  seed: { kind: state, source: src/StatePick.java#StatePick.seed, type: ChoiceBase }
T:
  - { id: $t_0, op: member_choice, when: 'true', members: values, empty: E.seed.value }
R: $t_0
```

## 空候选复用已经发生的外部整数回执

```java
import normalizer.annotations.External;

class ReceiptPick {
    @External private final Checkpoints checkpoints;

    ReceiptPick(Checkpoints checkpoints) { this.checkpoints = checkpoints; }

    int choose(int[] values, int code) {
        int first = checkpoints.read(code);
        int second = checkpoints.read(first);
        int selected;
        /* @normalizer-ir selected = choose from values else second */
        return selected;
    }
}

interface Checkpoints { int read(int code); }
```

**原样通过。** 第一个回执作为第二次真实调用的参数，两次调用都保留。空数组只使用第二个回执作为缺省值，不重发请求，不把候选选择计为外部调用。此处两个端口结果都是无异常、无条件取得的已声明整数；条件调用不由本例自动放行。

```yaml
schema: normalizer.pers/1
entry: { function: src/ReceiptPick.java#ReceiptPick.choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    code: { position: 1, type: number }
S:
  - call: { external: checkpoints.read, args: [code], returns: number, result: '$e_0' }
  - call: { external: checkpoints.read, args: ['$e_0'], returns: number, result: '$e_1' }
T:
  - { id: $t_0, op: member_choice, when: 'true', members: values, empty: '$e_1' }
R: $t_0
```


结果公式中严格整数加法可按公开的 E、T 结构键交换，`selected + second` 与 `second + selected` 都发布 `$e_1 + $t_0`；减法保留源码左右。若有 11 个真实回执，`$e_2` 在 `$e_10` 前，引用的公式排序不能重排 S 的调用链。


### 只在入口条件成立时发生的选择

如果 `backup == 0` 时执行第一次选择并直接返回，那么第二处选择只在 `backup != 0` 时发生。两个 T 分别记录互补的 `when`，R 用同一条件取对应的 T，不能为未发生的选择补造值。公开编号只证明引用的名字稳定；还必须证明该值在使用路径上已经取得。条件依赖另一次非确定选择时，当前入口明确拒绝而不是编造顺序或可用域。

## 同一 T 依赖图中的选择与严格递推

```java
class MixedTemporal {
    static int settle(int[] values, int backup) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int total;
        /* @normalizer-ir total = reduce values from picked as (acc, item) yield acc * 2 + item */
        return total + picked;
    }
}
```

**原样通过。** 递推的初值来自已取得的选择；先发布该选择，再发布依赖它的递推。选择和递推共享一次编号。调换相加的写法不改变编号或依赖；若反过来由选择使用递推结果作缺省值，则递推必须在前。没有依赖时纯递推按其结构键取稳定顺序，实际选择之间仍按发生顺序。只在部分路径取得的选择不允许直接作为无条件递推初值。 若再加入以递推结果为缺省值的选择，三个节点按传递依赖排序，不能因为只返回最后节点就删掉前两者。仅在 R 的某个分支读取一个已无条件完成的递推，不改变它与先前选择的取得域。未被 R 或其他定义使用的纯递推可删除，不能一并删去已经发生并被返回的选择。

```yaml
schema: normalizer.pers/1
entry: { function: src/MixedTemporal.java#MixedTemporal.settle, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    backup: { position: 1, type: number }
T:
  - { id: $t_0, op: member_choice, when: 'true', members: values, empty: backup }
  - { id: $t_1, op: fold, type: number, source: values, initial: '$t_0', accumulator: acc, element: item, body: 'acc * 2 + item' }
R: '$t_0 + $t_1'
```

若新增已取得的布尔参数 `enabled` 并仅在 `!enabled` 时选择，则 T 中只记录一条 `when: NOT (enabled)` 的选择。enabled 为真时直接返回备用值，不执行该选择；enabled 为假时返回该选择，空数组也仍取原备用值。
