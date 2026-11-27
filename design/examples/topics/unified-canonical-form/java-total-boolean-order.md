# 总布尔同类项：排列与括号不改变一次判断

## 设计意图

检验已取得的总布尔项按统一顺序折叠，互补换位不改变案例。不同变量不是互补；惰性、未取得或受短路保护的条件不能照搬无条件排序。

已取得的布尔值上，`(address ? consent : !consent) ? true : address` 应化简为 `address OR NOT (consent)`：address 为真时两条路都返回真，为假时只取 consent 的否定。原有 balance 仍是入参；不能因为式子不依赖它就改签名。

## 输入原始代码

三个输入都是原生布尔值，已在入口取得。判断发货须同时满足三个条件；只读这些输入，不调用服务，也不观察异常。

```java
class CanonicalChecks {
    static boolean approved(boolean address, boolean balance, boolean consent) {
        return (balance && address) && consent;
    }
}
```

## AI 预处理结果

**原样通过。** AI 不替内核打平或排序这些原生布尔条件。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalChecks.java#CanonicalChecks.approved, returns: bool }
P:
  parameters:
    address: { position: 0, type: bool }
    balance: { position: 1, type: bool }
    consent: { position: 2, type: bool }
R: address AND balance AND consent
```

三个原生 Boolean 都是总值；展开同类节点后按业务路径升序，固定左折叠。同值重复可以删除。OR 是另一种可交换的总布尔运算，换成 OR 后也按相同顺序排列，但值不同：只有 consent 为真时，AND 假、OR 真。此种子不许对尚未取得、受短路保护的观察作同样改写；见[短路种子](../control-flow/java-short-circuit.md)。

## 布尔互补表达式必须选同一个 CASE

两个已取得的整数都为零时选择 10，其他情况选择 20。两个条件均为总值；两种写法都观察相同输入。按[统一定形](../../../chapters/08-统一定形.md)两个互补方向整式复杂度相同，根运算符 AND 排在 OR 前，因此保留下面的合取作为 WHEN。

```java
class CanonicalChecks {
    static int tier(int a, int b) {
        if (a == 0 && b == 0) return 10;
        return 20;
    }
}
```

**原样通过。** 不预先交换两个结果臂。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalChecks.java#CanonicalChecks.tier, returns: number }
P:
  parameters:
    a: { position: 0, type: number }
    b: { position: 1, type: number }
R: |-
  CASE
    WHEN a == 0 AND b == 0 THEN 10
    ELSE 20
  END
```

## 不相邻的同值分支只合并各自真正命中的域

```java
class CanonicalChecks {
    static int priority(boolean a, boolean b, boolean c) {
        if (a) return 10;
        if (b) return 20;
        if (c) return 10;
        return 30;
    }
}
```

**原样通过。** 当 a 为假而 b、c 都为真时，先命中 b，应返回 20。不能直接把两个返回 10 的原条件写成 `a OR c`；只有 `b` 失败时第三个条件才有机会命中。10 的有效域是 `a OR (c AND NOT (b))`，20 的有效域是 `b AND NOT (a)`。按[纯布尔多臂 CASE](../../../chapters/08-统一定形.md#cf-11-纯布尔多臂-case-从有效域选形)比较所有结果排列及默认值：以 30 作默认值的两棵整式都需三个布尔运算；其他默认值至少四个。两棵同成本候选中，20 在前的首 WHEN 更简单，后续的 10 臂只需 `a OR c`。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalChecks.java#CanonicalChecks.priority, returns: number }
P:
  parameters:
    a: { position: 0, type: bool }
    b: { position: 1, type: bool }
    c: { position: 2, type: bool }
R: |-
  CASE
    WHEN b AND NOT (a) THEN 20
    WHEN a OR c THEN 10
    ELSE 30
  END
```

派生验收再比较两个覆盖边界：若默认也返回 10，只有 `a` 为假且 `b` 为真时才返回 20，`c` 不再影响数值。若保留默认 30 但改写成前面 `c` 失败后再判断 `!c`，末尾不同值 40 不可达；原 10、20、30 的有效域和首命中优先级不应改变。两种输入均只有原 P，没有 E、S 或异常。

## 两个分支的公因子与互补余项

四个输入都是已取得的原生布尔值。第一项和第三项同有 `common`，余下的 `first && second` 与 `!first || !second` 互补；中间的 `other` 不依赖这三个值。整体唯一结果为 `common OR other`。不能把只有同时为真与只有部分为假的两种分支当成额外的业务事件。

```java
class CanonicalChecks {
    static boolean remainder(boolean common, boolean first, boolean second, boolean other) {
        return (common && first && second) || (other || (common && (!first || !second)));
    }
}
```

**原样通过。** AI 不预先提取公因子或判断两个余项是否互补。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalChecks.java#CanonicalChecks.remainder, returns: bool }
P:
  parameters:
    common: { position: 0, type: bool }
    first: { position: 1, type: bool }
    second: { position: 2, type: bool }
    other: { position: 3, type: bool }
R: common OR other
```

**对照**　把两个余项分别换成 `first` 与 `!first`，结果相同。若改成 `first` 与 `!second`，则不互补：例如 `common=true, first=false, second=true, other=false` 时原式为假，直接化成 `common OR other` 却为真。

### 互补判定的边界

结构相等不是运行后相等。内部表达式比较必须区分数值一元负号与布尔 NOT；不会用哈希替代运算符比较。内部覆盖证明器也必须在 `[p, NOT(p)]` 与反序上都识别互补，而不同变量 `[p, NOT(q)]` 不互补。这两项证明都不依赖输入顺序或某个前置归约先成功。

合取左边若是受保护的存在性观察或部分布尔 CASE，不能把它当成任意路径上都可用的事实。已取得的普通布尔值可以在相同有效域内归约。

对 CASE 还要区分三种情况：部分 CASE 不产生一个新的正常返回（包括所有 guard 为假）；`p` 已失败时才可从后续的 `p OR q` 去掉 `p`；两个互补子域归并同值后，分组阶段仍保留总 CASE 的明确默认臂，之后才能凭完整覆盖证明将默认臂判为不可达。不把证明过程中的中间形式当成最终 PERS。

把其中任一合法的已合并写法作为输入，即使改为以 10 作默认值，也要回到同一输出；首命中的真实结果域才是候选生成的输入。公因子对照把 `common` 重复一次，仍只保留一个因子，合取和析取都要保留公因子的真实取得范围。

其他反例包括不同文本、换边后的互补比较和并不依赖彼此的区间。确认的 null 结果只有在函数签名声明该输入位置会触发、且极性匹配时才可替代显式空判断。对关系值，`Count > 0` 才能证明同一快照存在，`Count > -1` 则不能；空表只必然使它自身的选择为空，不推出另一独立表为空。结构识别按[统一定形](../../../chapters/08-统一定形.md)，不用比较最终文本来猜取得域。

部分 CASE 的非相邻同值臂只有在中间臂的优先级和各自取得域不变时才能合并；合并不会补出一个 ELSE。该例用 `p AND q` 和 `p AND NOT q` 共用 10，中间 `r` 先返回 20；所有条件都不命中时仍无正常结果。

能否展开布尔结构，取决于 CASE 是否全域有值，以及 EXISTS 是否只在特定路径取得。同一个嵌套 CASE 的互补结果可复用精确覆盖缓存，但换掉分支里的变量后不能沿用原结论。含 CASE 的比较也不能只根据变量名不相交就当作自由布尔原子。独立真值表和整式节点都有资源上限：超出时拒绝证明，而不是把未知当成成立。不得为了定形改动源码的短路与观察边界。

超过证明预算时不报告「已证明成立」，也不把未求出的结果当成反例。若因此无法完成已承诺的定形，则报告未完成，不把原形当规范输出。

如果有序 CASE 的第一臂条件是 `NOT (p)`，到达第二臂即可知道 `p`。第二臂的 `p AND q` 可以写成 `q`，其值和末尾兜底都不变。若第一臂是独立的 `r`，则没有此证明，不能去掉 `p`。

证明预算不足只表示这轮没有得到结论，不是找到反例。对同一范围，例如「`x > 4 AND x > 2` 是否推出 `x > 3`」，证明器预算用完时不作该化简；换成有预算的请求仍可证明，不能复用上次的“没有证明”作为否定答案。 若缺少此证明会阻断已承诺的定形，则最终报告未完成。

**相关入参对照**　只在 `a == b` 的父域内，将局部 CASE 的 `b AND c → 10，b → 20，ELSE 30` 改用 a，结果不变：两者都应定为 `CASE WHEN a AND c THEN 10 WHEN a THEN 20 ELSE 30 END`。例如 `(a,b,c) = (true,true,false)` 得 20，而 `(false,false,false)` 得 30，不能因两个输入无法在该域内单独翻转而删掉它们。
