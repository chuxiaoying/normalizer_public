# 总布尔同类项：排列与括号不改变一次判断

## 设计意图

检验已取得的总布尔项按统一顺序折叠，互补换位不改变案例。不同变量不是互补；惰性、未取得或受短路保护的条件不能照搬无条件排序。

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

两个已取得的整数都为零时选择 10，其他情况选择 20。两个条件均为总值；两种写法都观察相同输入。按[二分 CASE 的结构键](../../../unified-canonical-form.md#等价候选复杂度不变与唯一代表)比较 AND 与其 De Morgan 补的 OR，使用 AND 取向；不是由宿主看到一次取反就决定写 OR。

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

**原样通过。** 当 a 为假而 b、c 都为真时，先命中 b，应返回 20。不能直接把两个返回 10 的原条件写成 `a OR c`；只有 `b` 失败时第三个条件才有机会命中。合并后的 10 域是 `a OR (NOT b AND c)`，20 的有效域是 `NOT a AND b`，原默认值仍在最后。

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
    WHEN a OR NOT (b) AND c THEN 10
    WHEN NOT (a) AND b THEN 20
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

### 内核结构与互补证明的补充验收

结构相等不是运行后相等。内部表达式比较必须区分数值一元负号与布尔 NOT；不会用哈希替代运算符比较。内部覆盖证明器也必须在 `[p, NOT(p)]` 与反序上都识别互补，而不同变量 `[p, NOT(q)]` 不互补。程序源码会先做常量归约，证明器也可能先给条件排序；因此两项均用内核单测直接观察原始表达式和条件顺序。

单测另检查合取左边确实是存在性观察或部分 Boolean CASE 时的事实快路径：应拒绝把它们塞进简单事实表，不凭这个拒绝推出其他谓词。已取得的两个普通 Boolean 则仍可直接提取。源码执行器可能在合流前分解存在性和部分返回域，所以这里固定原始节点形态；不补虚构的无取得输入。

CASE 的内核单测另外区分三处中间结果：部分 CASE 不产生一个新的正常返回（包括所有 guard 为假）；`p` 已失败时才可从后续的 `p OR q` 去掉 `p`；两个互补子域归并同值后，分组阶段仍保留总 CASE 的明确默认臂，之后才能凭完整覆盖证明将默认臂判为不可达。源执行器已经执行首命中规则，通常不会原样发出这些中间节点。
