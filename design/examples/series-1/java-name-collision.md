# Hello World：入口参数与实例字段同名

## 输入原始代码

计算新额度与当前额度的差，再加上调整量。入口参数和实例字段都叫 `quota`，两份额度在本例中均非空。

```java
class Quota {
    short units;
}

class QuotaService {
    Quota quota;

    int difference(Quota quota, short adjustment) {
        return quota.units - this.quota.units + adjustment;
    }
}
```

## AI 预处理结果

本例约定把入口已有的 `QuotaService.quota` 作为业务状态观察。预处理在该字段的声明上加一次 `@EntryState`。同类型的形参 `quota` 仍是普通输入，方法中的两次读取沿用原 Java 写法。

```diff
--- a/src/QuotaService.java
+++ b/src/QuotaService.java
@@ -1,8 +1,11 @@
+import normalizer.annotations.EntryState;
+
 class Quota {
     short units;
 }

 class QuotaService {
+    @EntryState
     Quota quota;

     int difference(Quota quota, short adjustment) {
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 将隐式 `this` 显式化为静态入口的 `receiver`，并在语义工程中读取 `receiver.quota.units`；new 保留原方法，只在 `QuotaService.quota` 的声明上加 `@EntryState`。 | 业务状态必须主动标记。类型、字段名和 `this` 都不能自动决定状态边界；标记一次即可让原生代码继续承担实际读写。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Quota:
    fields:
      units: number

entry:
  function: src/QuotaService.java#QuotaService.difference
  returns: number

P:
  parameters:
    quota:
      position: 0
      type: Quota
    adjustment:
      position: 1
      type: number

E:
  quota:
    kind: state
    source: src/QuotaService.java#QuotaService.quota
    type: Quota

R: adjustment + (P.quota.units - E.quota.units)
```

入口参数和实例字段共用 `types.Quota` 中的字段定义。源码里的 `quota.units` 对应 `P.quota.units`，`this.quota.units` 对应 `E.quota.units`。`adjustment` 只在 P 中声明，直接引用。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old R 通过 E 的编号区分两份额度；new 直接写 `P.quota.units` 与 `E.quota.units`，未冲突的 `adjustment` 仍不加前缀。 | 来源清楚时使用自然名称，只有冲突时才补最小限定，在无歧义与可读性之间保持一致规则。 |
| old 的规范项目运行退出 0、有 PERS，但输出覆盖了更宽的载体域：`receiver` 可空，承载 `short` 原值的 `i32` 可超出原值域。对照原入口时必须限定 `receiver` 为原非空 `this`，`units` 与 `adjustment` 为 -32768 到 32767；有效映射域内保持原语义。new 预期保留原入口和主动标记的状态，不引入额外接收者参数。 | 原入口与分析载体必须按有效映射域比较。成功生成载体的结果，不意味着载体额外允许的输入和异常也是原业务行为。 |
