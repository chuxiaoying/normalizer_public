# Hello World：Vue VO 消费归一化

## 设计意图

变化轴是同一 VO 的组件出口与字段依赖。金额和说明分别观察各自的绑定与派生值；改无关字段、私有变量名不应污染另一个出口，改实际绑定才改变相应 PERS。

准备模块和独立 TypeScript 共用严格检查。显式、隐式或由库声明推导的 `any` 都不能交付，包括未调用的辅助函数和未使用的声明；禁止关闭或压制检查的编译注释。已经证明的关系指令初始化只补足其实际赋值证据，不豁免其他类型检查。

字段名按每次成员访问识别：`arrears.summary.payable` 有两级字段，`arrears["summary"]["payable"]` 访问相同字段，PERS 必须相同；`arrears["summary.payable"]` 却只查一个含点的键，不能误当成嵌套读取。若这个键未声明，适配入口应报缺少该字段，不应伪造 E 或 R。若实际声明了含点键，它仍是独立的合法 JS 键；如何在 PERS 中无歧义地表示这样的字段不由本种子决定。

状态注解只为一个具名源绑定确定 E 身份。准备后的组件投影必须是同步、导出的无参函数；不能给未经声明的形参捏造 P，也不能在同一注解下任选一个变量。

## 输入原始代码

欠费页把同一份 VO 投影到金额卡片的两个动态属性。一次消费观察的是一次渲染求值时某个属性收到的值。下面给出页面的完整消费切片。模板中的静态 `id="balance"` 用来辨认这个组件实例。

```vue
<template>
  <PayableCard
    id="balance"
    :amount="arrears.status.ready ? arrears.summary.payable : 0"
    :caption="arrears.status.loading ? '加载中' : '应付金额'"
  />
</template>

<script setup>
import { reactive } from 'vue'
import PayableCard from './PayableCard.vue'

const arrears = reactive({
  status: { ready: false, loading: false },
  summary: { payable: 0 }
})
</script>
```

## AI 预处理结果

两个模板绑定各提取为一个无参函数，`@N componentProp` 保留它在源模板中的位置；PERS 的入口是分析模块中实际可调用的导出函数，而不是模板位置。下列模块描述单个页面实例：setup 创建一次 `arrears`，以后每次渲染都从这个实例已有的状态求值。

```diff
diff --git a/src/ArrearsPage.vue b/src/ArrearsPage.ts
rename from src/ArrearsPage.vue
rename to src/ArrearsPage.ts
--- a/src/ArrearsPage.vue
+++ b/src/ArrearsPage.ts
@@ -1,17 +1,15 @@
-<template>
-  <PayableCard
-    id="balance"
-    :amount="arrears.status.ready ? arrears.summary.payable : 0"
-    :caption="arrears.status.loading ? '加载中' : '应付金额'"
-  />
-</template>
-
-<script setup>
-import { reactive } from 'vue'
-import PayableCard from './PayableCard.vue'
-
-const arrears = reactive({
+/** @N entryState */
+const arrears = {
   status: { ready: false, loading: false },
   summary: { payable: 0 }
-})
-</script>
+};
+
+/** @N componentProp PayableCard#balance.amount */
+export function amount(): number {
+  return arrears.status.ready ? arrears.summary.payable : 0;
+}
+
+/** @N componentProp PayableCard#balance.caption */
+export function caption(): string {
+  return arrears.status.loading ? "加载中" : "应付金额";
+}
```

**预处理要点**：提取入口可以改变分析载体，却不能扩大业务接口。理想交接直接保留既有实例上下文和属性身份，减少适配层再证明“这个参数实际绑定谁”的负担。

## 归一化最终结果

以下为两个入口各自的目标 PERS。数值字段 `payable` 和金额属性都保持输入声明能确定的 `number`。

### `PayableCard#balance.amount`

```yaml
schema: normalizer.pers/1
entry:
  function: src/ArrearsPage.ts#amount
  returns: number
P:
  parameters: {}
E:
  arrears:
    kind: state
    source: src/ArrearsPage.ts#arrears
    fields:
      status: { ready: bool }
      summary: { payable: number }
R: |-
  CASE WHEN arrears.status.ready THEN arrears.summary.payable ELSE 0 END
```

### `PayableCard#balance.caption`

```yaml
schema: normalizer.pers/1
entry:
  function: src/ArrearsPage.ts#caption
  returns: text
P:
  parameters: {}
E:
  arrears:
    kind: state
    source: src/ArrearsPage.ts#arrears
    fields:
      status: { loading: bool }
R: |-
  CASE WHEN arrears.status.loading THEN "加载中" ELSE "应付金额" END
```

**PERS 验收要点**

- 统一函数入口仍须保留它代表的模板属性身份。提取函数只是分析手段，业务观察的对象始终是原组件属性及其读取的状态。
- 不能以缩窄输入数值域换取成功输出。业务数值抽象的目标见[基础案例](../expression-simplification/java-static.md)；无法闭合的适配应明确报告。
- 源码到结果的验收要贯穿原入口和上下文约束。CLI 有输出不代表原行为已被完整承接；适配参数的技术取值范围不能冒充真实业务输入域。

数值域的共性见[基础案例](../expression-simplification/java-static.md)，业务类型默认非空的共性见[对象案例](../objects-and-types/java-static-object.md)，状态身份的共性见[参数与状态同名案例](../basic-contracts/java-name-collision.md)。

[第七章的对应原例](../../../chapters/07-PERS登场.md)：第七章的属性入口直接验收原例：无参数、无写入，只读取该次求值开始前的页面状态。
