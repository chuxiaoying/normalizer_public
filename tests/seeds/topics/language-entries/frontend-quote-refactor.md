# Hello World：从完整前端页面提取一次报价刷新

## 设计意图

一个页面同时显示报价、刷新报价和切换帮助。这里不观察一次完整渲染，也不观察另一个按钮；入口是一次报价刷新，观察两个状态字段收到什么以及写入顺序。页面已绑定的 `units` 和 `expedited` 是这次动作取得的业务值，帮助状态不是这个入口的输入或产物。

## 输入原始代码

`src/QuotePanel.vue`：

```vue
<template>
  <aside>
    <button @click="refreshQuote">更新报价</button>
    <strong v-if="quote.display.active">{{ quote.display.amount }}</strong>
    <button @click="toggleHelp">帮助</button>
    <p v-if="help.open">加急报价另计费用。</p>
  </aside>
</template>

<script setup lang="ts">
import { reactive } from "vue";

const props = defineProps<{units: number; expedited: boolean}>();
const quote = reactive({display: {amount: 0, active: false}});
const help = reactive({open: false});

function refreshQuote() {
  const amount = props.expedited ? props.units + 10 : props.units;
  quote.display.amount = amount;
  quote.display.active = props.units > 0;
}

function toggleHelp() {
  help.open = !help.open;
}
</script>
```

## AI 预处理结果

把 `refreshQuote` 的源码取出为严格 TypeScript。把本次动作稳定取得的两个值写成形参；保留现存 `quote`、局部计算和两次写入。模板、帮助按钮和宿主响应式包装不在观察范围内，移除它们不会替内核预先算出金额。

```diff
--- a/src/QuotePanel.vue
+++ b/src/QuotePanel.ts
@@ -1,26 +1,8 @@
-<template>
-  <aside>
-    <button @click="refreshQuote">更新报价</button>
-    <strong v-if="quote.display.active">{{ quote.display.amount }}</strong>
-    <button @click="toggleHelp">帮助</button>
-    <p v-if="help.open">加急报价另计费用。</p>
-  </aside>
-</template>
+/** @N entryState */
+const quote = {display: {amount: 0, active: false}};

-<script setup lang="ts">
-import { reactive } from "vue";
-
-const props = defineProps<{units: number; expedited: boolean}>();
-const quote = reactive({display: {amount: 0, active: false}});
-const help = reactive({open: false});
-
-function refreshQuote() {
-  const amount = props.expedited ? props.units + 10 : props.units;
+export function refreshQuote(units: number, expedited: boolean): void {
+  const amount = expedited ? units + 10 : units;
   quote.display.amount = amount;
-  quote.display.active = props.units > 0;
+  quote.display.active = units > 0;
 }
-
-function toggleHelp() {
-  help.open = !help.open;
-}
-</script>
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/QuotePanel.ts#refreshQuote, returns: void }
P:
  parameters:
    units: { position: 0, type: number }
    expedited: { position: 1, type: bool }
E:
  quote:
    kind: state
    source: src/QuotePanel.ts#quote
    fields: { display: { amount: number, active: bool } }
S:
  - write: { target: quote.display.amount, value: CASE WHEN expedited THEN units + 10 ELSE units END }
  - write: { target: quote.display.active, value: units > 0 }
R: void
```

加急只改变第一笔写入的值，第二笔始终根据这次取得的数量；帮助状态即使另有导出动作也不进入这个 PERS。若把两笔状态写入调换，PERS 的 S 顺序也必须调换，不能按字段名排序。
