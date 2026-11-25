# Hello World：Vue VO 生产归一化

## 输入原始代码

入口是一个页面实例的 `onLoad()`。观察从它记录账户、开启加载状态开始，到两次外部请求成功并写入应付金额，或某次请求失败、清理状态并传播异常为止。模板展示和 `onMounted` 注册均在这次函数调用之外。本例两个请求返回独立的普通业务数据值，返回后不会被外部继续修改；字段读取没有 getter 副作用。

```vue
<template>
  <section v-if="arrears.status.ready">
    <strong>{{ arrears.summary.payable }}</strong>
  </section>
  <p v-else-if="arrears.status.loading">Loading</p>
  <p v-else-if="arrears.status.failed">Failed</p>
</template>

<script setup lang="ts">
import { onMounted, reactive } from "vue"

type BillRow = {
  accountId: number
  dueDate: number
  amount: number
  waivedAmount: number
  priority: number
}

type Profile = {
  accountId: number
  minPriority: number
  quota: number
}

class ProfileFetchError extends Error {}
class BillQueryError extends Error {}

const arrears = reactive({
  loadedAccountId: 0,
  status: {
    loading: false,
    ready: false,
    failed: false,
  },
  summary: {
    payable: 0,
  },
})

const props = defineProps<{
  accountId: number
  today: number
}>()

interface ProfileClient {
  /** rejects with ProfileFetchError */
  fetchProfile(accountId: number): Promise<Profile>
}

interface ArrearsClient {
  /** rejects with BillQueryError */
  queryOpenBills(accountId: number): Promise<BillRow[]>
}

declare const profileClient: ProfileClient
declare const arrearsClient: ArrearsClient

async function onLoad() {
  arrears.loadedAccountId = props.accountId
  arrears.status.loading = true

  try {
    const profile = await profileClient.fetchProfile(props.accountId)
    const rows = await arrearsClient.queryOpenBills(profile.accountId)

    const total = rows
      .filter(row => row.dueDate <= props.today)
      .filter(row => row.priority >= profile.minPriority)
      .map(row => row.amount - row.waivedAmount)
      .filter(netAmount => netAmount > 0)
      .reduce((sum, netAmount) => sum + netAmount, 0)

    arrears.status.loading = false
    arrears.summary.payable = total > profile.quota ? profile.quota : total
    arrears.status.ready = true
  } catch (error) {
    arrears.status.loading = false
    arrears.status.failed = true
    arrears.status.ready = false
    throw error
  }
}

// onLoad is the scenario entry. This call is lifecycle wiring outside that entry.
onMounted(() => onLoad())
</script>
```

## AI 预处理结果

以下 diff 的输入是上面完整的 `ArrearsPage.vue`，输出是普通 ECMAScript 模块 `ArrearsPage.mjs`。保留已有的 `onLoad()` 并导出，去掉入口外的模板和生命周期注册。原 TypeScript 声明用 JSDoc 表达，JavaScript 解析器可以直接读取整个模块。

```diff
diff --git a/src/ArrearsPage.vue b/src/ArrearsPage.mjs
rename from src/ArrearsPage.vue
rename to src/ArrearsPage.mjs
--- a/src/ArrearsPage.vue
+++ b/src/ArrearsPage.mjs
@@ -1,31 +1,23 @@
-<template>
-  <section v-if="arrears.status.ready">
-    <strong>{{ arrears.summary.payable }}</strong>
-  </section>
-  <p v-else-if="arrears.status.loading">Loading</p>
-  <p v-else-if="arrears.status.failed">Failed</p>
-</template>
+/**
+ * @typedef {object} BillRow
+ * @property {number} accountId
+ * @property {number} dueDate
+ * @property {number} amount
+ * @property {number} waivedAmount
+ * @property {number} priority
+ */

-<script setup lang="ts">
-import { onMounted, reactive } from "vue"
-
-type BillRow = {
-  accountId: number
-  dueDate: number
-  amount: number
-  waivedAmount: number
-  priority: number
-}
-
-type Profile = {
-  accountId: number
-  minPriority: number
-  quota: number
-}
+/**
+ * @typedef {object} Profile
+ * @property {number} accountId
+ * @property {number} minPriority
+ * @property {number} quota
+ */

 class ProfileFetchError extends Error {}
 class BillQueryError extends Error {}

-const arrears = reactive({
+/** @entryState */
+const arrears = {
   loadedAccountId: 0,
   status: {
@@ -37,25 +29,37 @@
     payable: 0,
   },
-})
-
-const props = defineProps<{
-  accountId: number
-  today: number
-}>()
-
-interface ProfileClient {
-  /** rejects with ProfileFetchError */
-  fetchProfile(accountId: number): Promise<Profile>
 }

-interface ArrearsClient {
-  /** rejects with BillQueryError */
-  queryOpenBills(accountId: number): Promise<BillRow[]>
-}
+/**
+ * @entryInputs
+ * @type {{ accountId: number, today: number }}
+ */
+let props

-declare const profileClient: ProfileClient
-declare const arrearsClient: ArrearsClient
+/**
+ * @callback FetchProfile
+ * @param {number} accountId
+ * @returns {Promise<Profile>}
+ * @rejects {ProfileFetchError}
+ */
+/**
+ * @callback QueryOpenBills
+ * @param {number} accountId
+ * @returns {Promise<BillRow[]>}
+ * @rejects {BillQueryError}
+ */

-async function onLoad() {
+/**
+ * @external
+ * @type {{ fetchProfile: FetchProfile }}
+ */
+let profileClient
+/**
+ * @external
+ * @type {{ queryOpenBills: QueryOpenBills }}
+ */
+let arrearsClient
+
+export async function onLoad() {
   arrears.loadedAccountId = props.accountId
   arrears.status.loading = true
@@ -65,10 +69,14 @@
     const rows = await arrearsClient.queryOpenBills(profile.accountId)

-    const total = rows
-      .filter(row => row.dueDate <= props.today)
-      .filter(row => row.priority >= profile.minPriority)
-      .map(row => row.amount - row.waivedAmount)
-      .filter(netAmount => netAmount > 0)
-      .reduce((sum, netAmount) => sum + netAmount, 0)
+    /** @type {number} */
+    let total
+    /* @normalizer-ir
+       total = sum from rows as row
+         where row.dueDate <= props.today
+           and row.priority >= profile.minPriority
+         let netAmount = row.amount - row.waivedAmount
+         where netAmount > 0
+         yield netAmount
+    */

     arrears.status.loading = false
@@ -82,6 +90,2 @@
   }
 }
-
-// onLoad is the scenario entry. This call is lifecycle wiring outside that entry.
-onMounted(() => onLoad())
-</script>
```

`@entryState` 标明 `arrears` 是当前实例已创建的状态。`@entryInputs` 标明 props 来自宿主，`accountId` 在第一次等待前用于记录账户和请求资料。`today` 是本次调用的不变量，即使源码在等待后才用它筛选，也仍使用本次调用绑定的值。两个 `@external` 声明给出各自的返回值和拒绝类型。

集合链在原处改写为局部聚合。该表达以行字段与筛选期间读取的输入稳定、读取无可观察副作用为前提；若 getter 的调用次序可被观察，仍须保留原链分阶段读取的顺序。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 的 TypeScript 候选融合多轮筛选，改变了字段读取顺序；等价所需的数据与输入稳定性、字段读取无副作用等前提未闭合，不能作为已验收预处理交付；new 预期交付原生 JavaScript 模块，以 JSDoc 和 normalizer 标记保留声明，以局部聚合替换集合链；两次 `await`、状态写入和异常清理留在原入口中。 | 框架预处理必须交付工程真正能承接的业务入口。去掉框架外壳之后，仍要为工程不能推导的集合语义建立明确交接；转换载体时保留等待关系与成功、失败路径。 |

集合局部替换的共性见[关系聚合案例](java-loop-fold.md)；状态主动标记的共性见[参数与状态同名案例](java-name-collision.md)。

## 归一化最终结果

以下为 `onLoad` 的目标 PERS。JSDoc 将金额、日期和账户编号声明为 `number`，这些字段在最终 PERS 中保留同一数值类型。P 保留 `@entryInputs` 声明的两个宿主输入，`position` 表示其声明顺序；`accountId` 是首次等待前读取的账户；`today` 是本次调用绑定的业务输入，等待前后相同。新日期只能作为下一次调用的输入。这里的约定不适用于可变的宿主状态或外部观察。`$e_0` 与 `$e_1` 分别绑定本例两个端口返回的业务值。

~~~yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      accountId: number
      minPriority: number
      quota: number
  BillRow:
    fields:
      accountId: number
      dueDate: number
      amount: number
      waivedAmount: number
      priority: number

entry:
  function: src/ArrearsPage.vue#onLoad
  returns: void

P:
  parameters:
    accountId:
      position: 0
      type: number
    today:
      position: 1
      type: number

E:
  arrears:
    kind: state
    source: src/ArrearsPage.vue#ArrearsPage.arrears
    fields:
      loadedAccountId: number
      status:
        loading: bool
        ready: bool
        failed: bool
      summary:
        payable: number

S:
  - write:
      target: arrears.loadedAccountId
      value: accountId

  - write:
      target: arrears.status.loading
      value: true

  - call:
      external: profileClient.fetchProfile
      args:
        - accountId
      returns: Profile
      result: $e_0

  - call:
      external: arrearsClient.queryOpenBills
      args:
        - $e_0.accountId
      returns: BillRow[]
      result: $e_1
      when: completion.is_return($e_0)

  - write:
      target: arrears.status.loading
      value: false

  - write:
      target: arrears.summary.payable
      value: |-
        CASE
          WHEN $e_0.quota < COALESCE(
            SUM(
              SELECT row.amount - row.waivedAmount
              FROM $e_1 AS row
              WHERE row.dueDate <= today
                AND $e_0.minPriority <= row.priority
                AND row.amount > row.waivedAmount
            ),
            0
          ) THEN $e_0.quota
          ELSE COALESCE(
            SUM(
              SELECT row.amount - row.waivedAmount
              FROM $e_1 AS row
              WHERE row.dueDate <= today
                AND $e_0.minPriority <= row.priority
                AND row.amount > row.waivedAmount
            ),
            0
          )
        END
      when: completion.is_return($e_0) and completion.is_return($e_1)

  - write:
      target: arrears.status.ready
      value: true
      when: completion.is_return($e_0) and completion.is_return($e_1)

  - write:
      target: arrears.status.failed
      value: true
      when: completion.is_throw($e_0, ProfileFetchError) or completion.is_throw($e_1, BillQueryError)

  - write:
      target: arrears.status.ready
      value: false
      when: completion.is_throw($e_0, ProfileFetchError) or completion.is_throw($e_1, BillQueryError)

R: void

throws:
  - type: BillQueryError
    when: completion.is_return($e_0) and completion.is_throw($e_1, BillQueryError)
  - type: ProfileFetchError
    when: completion.is_throw($e_0, ProfileFetchError)
~~~

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 的候选经正式前端 CLI 运行，在统一 IR 构造阶段因输入长度决定的原生循环无法降低而退出 2；未进入内核，stdout 为空，没有生产 PERS。new 预期得到已有状态来源、两次调用的依赖关系、成功写入、失败清理，以及 `R: void` 和异常完成条件。 | 一次页面动作的成果要由状态变化、外部交互和完成结果共同表达。返回 `void` 不等于没有业务输出；产出源码或局部计算公式，也不能代替完整动作的归一化结果。 |

入口与输入声明的共性见[基础案例](java-static.md)，业务类型的共性见[对象案例](java-static-object.md)。
