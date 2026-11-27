# Hello World：Vue VO 生产归一化

## 设计意图

变化轴是事件入参、跨 await 的读取、条件调用和有序状态写入。today 取自入口且两次读取相同；回执、可变状态和宿主不同入口的字段不能借用这条不变量。

准备模块和独立 TypeScript 共用严格检查。显式、隐式或由库声明推导的 `any` 都不能交付，包括未调用的辅助函数和未使用的声明；禁止关闭或压制检查的编译注释。已经证明的关系指令初始化只补足其实际赋值证据，不豁免其他类型检查。

分析交付的声明不是装饰：普通说明注释不改变源合同或 PERS。等待端口必须有可解析的正常返回类型；有声明的失败才建立外部异常路径；本次调用的两个业务输入是具名必填形参。关系动作只能给当前块中可写的局部变量赋值，不能改写 const。无效准备输入在模拟执行前拒绝，不发布部分 PERS。

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
  const todayAtEntry = props.today
  arrears.loadedAccountId = props.accountId
  arrears.status.loading = true

  try {
    const profile = await profileClient.fetchProfile(props.accountId)
    const rows = await arrearsClient.queryOpenBills(profile.accountId)

    const total = rows
      .filter(row => row.dueDate <= todayAtEntry)
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

以下 diff 的输入是上面完整的 `ArrearsPage.vue`，输出是严格 TypeScript 分析模块 `ArrearsPage.ts`。导出 `onLoad`，将宿主本次交付的账户和日期写成类型明确的形参；去掉本次入口之外的模板和生命周期注册。

```diff
diff --git a/src/ArrearsPage.vue b/src/ArrearsPage.ts
rename from src/ArrearsPage.vue
rename to src/ArrearsPage.ts
--- a/src/ArrearsPage.vue
+++ b/src/ArrearsPage.ts
@@ -1,88 +1,74 @@
-<template>
-  <section v-if="arrears.status.ready">
-    <strong>{{ arrears.summary.payable }}</strong>
-  </section>
-  <p v-else-if="arrears.status.loading">Loading</p>
-  <p v-else-if="arrears.status.failed">Failed</p>
-</template>
-
-<script setup lang="ts">
-import { onMounted, reactive } from "vue"
-
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

-const arrears = reactive({
+/** @N entryState */
+const arrears = {
   loadedAccountId: 0,
   status: {
     loading: false,
     ready: false,
     failed: false,
   },
   summary: {
     payable: 0,
   },
-})
-
-const props = defineProps<{
-  accountId: number
-  today: number
-}>()
+}

 interface ProfileClient {
-  /** rejects with ProfileFetchError */
+  /** @N rejects ProfileFetchError */
   fetchProfile(accountId: number): Promise<Profile>
 }

 interface ArrearsClient {
-  /** rejects with BillQueryError */
+  /** @N rejects BillQueryError */
   queryOpenBills(accountId: number): Promise<BillRow[]>
 }

+/** @N external */
 declare const profileClient: ProfileClient
+/** @N external */
 declare const arrearsClient: ArrearsClient

-async function onLoad() {
-  const todayAtEntry = props.today
-  arrears.loadedAccountId = props.accountId
+export async function onLoad(accountId: number, today: number): Promise<void> {
+  const todayAtEntry = today
+  arrears.loadedAccountId = accountId
   arrears.status.loading = true

   try {
-    const profile = await profileClient.fetchProfile(props.accountId)
+    const profile = await profileClient.fetchProfile(accountId)
     const rows = await arrearsClient.queryOpenBills(profile.accountId)

-    const total = rows
-      .filter(row => row.dueDate <= todayAtEntry)
-      .filter(row => row.priority >= profile.minPriority)
-      .map(row => row.amount - row.waivedAmount)
-      .filter(netAmount => netAmount > 0)
-      .reduce((sum, netAmount) => sum + netAmount, 0)
+    let total: number
+    /* @normalizer-ir
+       total = sum from rows as row
+         where row.dueDate <= todayAtEntry
+           and row.priority >= profile.minPriority
+         let netAmount = row.amount - row.waivedAmount
+         where netAmount > 0
+         yield netAmount
+    */

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
-
-// onLoad is the scenario entry. This call is lifecycle wiring outside that entry.
-onMounted(() => onLoad())
-</script>
```

`@N entryState` 标明 `arrears` 属于已创建的页面实例。本次入口开始时，宿主交付账户和日期；形参 `accountId` 和 `today` 是本次调用的 P。源码在首次等待前把日期存入 `todayAtEntry`，等待后的筛选只读该局部值。若原入口在等待后重新观察可变的宿主状态，不能把那次观察提前或当作同一个 P。

两个 `@N external` 声明标记实际端口。源场景已有闭合的完成合同：`profileClient.fetchProfile` 成功返回 `Profile`、失败为 `ProfileFetchError`；`arrearsClient.queryOpenBills` 成功返回 `BillRow[]`、失败为 `BillQueryError`。分析副本用方法上的 `@N rejects ErrorType` 交付这份已核实的失败合同，本例的两个失败类型都可解析到随项目交付的异常类。仅凭 TypeScript 的 `Promise<T>` 签名不能推知拒绝类型，普通说明注释也不建立失败路径。

惰性条件的任一分支也可以引用这类调用不变量。归一化结果只选择真实分支的值：两个分支若引用同一个 P，结果就是该入参本身；等待后的直接读取与等待前保存的局部别名等价。读取 P 不产生新的 E 或 S，也不改变写入、调用和等待的顺序。该式只对已声明为调用不变量的 P 成立，不允许提前观察可变状态、外部回执或执行分支中的外部调用。

集合链在原处改写为局部聚合。该表达以行字段与筛选期间读取的输入稳定、读取无可观察副作用为前提；若 getter 的调用次序可被观察，仍须保留原链分阶段读取的顺序。

**预处理要点**：框架预处理必须交付工程真正能承接的业务入口。去掉框架外壳之后，仍要为工程不能推导的集合语义建立明确交接；转换载体时保留等待关系与成功、失败路径。

集合局部替换的共性见[关系聚合案例](../loops-and-termination/java-loop-fold.md)；状态主动标记的共性见[参数与状态同名案例](../basic-contracts/java-name-collision.md)。

## 归一化最终结果

以下为 `onLoad` 的目标 PERS。严格 TypeScript 将金额、日期和账户编号声明为 `number`，这些字段在 PERS 中保留同一数值类型。P 保留导出函数的两个形参，`position` 表示签名顺序；`accountId` 是首次等待前读取的账户；`today` 是本次调用绑定的业务输入，等待前后相同。新日期只能作为下一次调用的输入。这里的约定不适用于可变的宿主状态或外部观察。`$e_0` 与 `$e_1` 分别绑定本例两个端口返回的业务值。

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
  function: src/ArrearsPage.ts#onLoad
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
    source: src/ArrearsPage.ts#arrears
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
      external: src/ArrearsPage.ts#profileClient.fetchProfile
      args:
        - accountId
      returns: Profile
      result: $e_0

  - call:
      external: src/ArrearsPage.ts#arrearsClient.queryOpenBills
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
          WHEN $e_0.quota > COALESCE(
            SUM(
              SELECT row.amount - row.waivedAmount
              FROM $e_1 AS row
              WHERE today >= row.dueDate
                AND row.amount > row.waivedAmount
                AND $e_0.minPriority <= row.priority
            ),
            0
          ) THEN COALESCE(
            SUM(
              SELECT row.amount - row.waivedAmount
              FROM $e_1 AS row
              WHERE today >= row.dueDate
                AND row.amount > row.waivedAmount
                AND $e_0.minPriority <= row.priority
            ),
            0
          )
          ELSE $e_0.quota
        END
      when: |-
        CASE
          WHEN completion.is_return($e_0) THEN completion.is_return($e_1)
          ELSE false
        END

  - write:
      target: arrears.status.ready
      value: true
      when: |-
        CASE
          WHEN completion.is_return($e_0) THEN completion.is_return($e_1)
          ELSE false
        END

  - write:
      target: arrears.status.failed
      value: true
      when: |-
        CASE
          WHEN completion.is_return($e_0) THEN completion.is_throw($e_1, BillQueryError)
          ELSE true
        END

  - write:
      target: arrears.status.ready
      value: false
      when: |-
        CASE
          WHEN completion.is_return($e_0) THEN completion.is_throw($e_1, BillQueryError)
          ELSE true
        END

R: void

throws:
  - type: BillQueryError
    when: |-
      CASE
        WHEN completion.is_return($e_0) THEN completion.is_throw($e_1, BillQueryError)
        ELSE false
      END
  - type: ProfileFetchError
    when: completion.is_throw($e_0, ProfileFetchError)
~~~

应付金额在两个回执都已正常取得的域内取合计与额度的较小值。此例的业务合同约定回执中的额度与账单金额为有限、精确的业务数值，合计也在该数值域内；不包含 NaN、无穷或可观察的有符号零。这不是仅凭 TypeScript 的 `number` 推断。相等时两臂无区别；同成本的严格 CASE 由共享比较器选 `>`，不改变任何调用条件。

第 2 次调用只在第 1 次调用成功时发出。第 1 次失败时直接进入失败路径，不读取未取得的第 2 次回执；成功路径同样先检查第 1 次是否完成。

**失败声明的对照。** 若合同只声明第一个端口的成功类型，并从分析副本移除它的 `@N rejects ProfileFetchError`，按 [AI-09](../../../chapters/04-AI救赎.md#ai-09-外部异常须显式声明)两次调用仍保留。第一个端口在所建模的域内正常完成，第二次调用不再需要第一笔的成功守卫，但仍用第一笔回执的账户号。成功写入由第二笔正常完成保护；失败清理和顶层异常只剩第二笔的 `BillQueryError`。清理后原样重抛，不把错误换成另一种值。若只删分析副本的标记却丢掉了原业务合同明确需要观察的失败，这不是等价预处理。

**PERS 验收要点**：一次页面动作的成果要由状态变化、外部交互和完成结果共同表达。返回 `void` 不等于没有业务输出；产出源码或局部计算公式，也不能代替完整动作的归一化结果。

入口与输入声明的共性见[基础案例](../expression-simplification/java-static.md)，业务类型的共性见[对象案例](../objects-and-types/java-static-object.md)。
