# 第四章 · AI 圈地：哪些地方要继续算，哪些地方该停

一个 Spring controller 在正常请求里能调用注入好的 service。但如果只拿 controller 的 Java 源码来执行，模拟器看不到 Spring 什么时候创建了 service，又把它装到了哪里。**预处理要补的，就是这些原程序依赖、却没写在可执行源码里的步骤。**

交给模拟器的是一份**原语言的纯源码工程**。「纯」不是说程序不能改状态或调用外部服务，而是说：**从选定入口开始，凡是需要模拟器亲自执行的代码，都在原语言源码里看得见。** Java 仍用 Java 表达，Vue 里的业务逻辑可以抽成普通 JS 或 TS 模块。已经写清楚的原生程序直接交给工程，AI 不再改写一遍。

## 一、先把入口和运行环境写清楚

### Spring：在 Beans.java 装好对象，再调用入口

最简单的入口是一个 Java 静态方法，例如 `static int twice(int x) { return x * 2; }`。它不需要框架帮忙就能调用。

Spring 的实例方法要先有装配好的对象。比如 controller 调用 `price.quote(units)`；如果只 `new QuoteController()` 就执行，`price` 还没注入，会碰到一个正常 Spring 请求里不会发生的空指针。

可以把原本分散在 Spring 配置和注解里的装配，集中写进一个 **`Beans.java`**。先创建相应的实例，再把依赖 `set` 进去，全部装好之后才调用入口。下面是一个装配方案，还不是现有种子验收过的 Spring 转换。这两个 Bean 都是单例，原来用字段注入，新源码就把这次注入写成一次显式的赋值：

```diff
--- a/src/QuoteController.java
+++ b/src/QuoteController.java
@@ -1,19 +1,15 @@
-import org.springframework.beans.factory.annotation.Autowired;
-import org.springframework.stereotype.Service;
-import org.springframework.web.bind.annotation.*;
-
-@Service
 class PriceService {
     int quote(int units) { return units * 2; }
 }

-@RestController
 class QuoteController {
-    @Autowired
     private PriceService price;

-    @GetMapping("/quote")
-    public int quote(@RequestParam("units") int units) {
+    void setPrice(PriceService price) {
+        this.price = price;
+    }
+
+    public int quote(int units) {
         return price.quote(units);
     }
 }
--- /dev/null
+++ b/src/Beans.java
@@ -0,0 +1,10 @@
+final class Beans {
+    static final PriceService PRICE = new PriceService();
+    static final QuoteController QUOTE_CONTROLLER = new QuoteController();
+
+    static {
+        QUOTE_CONTROLLER.setPrice(PRICE);
+    }
+
+    private Beans() {}
+}
```

装好之后，入口就是对现成对象的一次调用：

```java
Beans.QUOTE_CONTROLLER.quote(units)
```

如果选的是 service 方法做入口，同样可以直接用 `Beans.PRICE.quote(units)`。对 controller 而言，这里从 **HTTP 请求参数已成功绑定** 开始：`units` 来自请求，`price` 来自 `Beans.java`，两者不能混。若要分析整个 HTTP 请求，绑定失败和校验也要补出来。如果工具只接收具名静态方法作为入口，可以用一个只转发这次调用的静态方法；装配仍只在 `Beans.java`。

项目里 Bean 多了，也用同一个结构：从选定入口出发，按实际生效的配置找出会影响它的 Bean，再把确认过的装配集中生成到 `Beans.java`，不用把全应用手抄一遍。这里的静态实例代表**当前分析的一个容器里的单例**；请求级对象仍按请求创建，原来要求构造器注入的也不用硬改成 setter。若鉴权、事务代理或初始化动作会改变结果，就要显式保留那一步，不能因为对象装上了就当它们不存在。

### 前端入口：动作、入口输入和实例状态别混在一起

这次分析的是「点击重置按钮」，不是挂载页面或渲染按钮。原来由 Vue 在用户点击时调用 `onReset`，而这个函数修改的是**当前组件本来就有的计数**。预处理把处理器导出，并用 `@entryState` 标出组件状态：

```diff
--- a/src/Counter.js
+++ b/src/Counter.js
@@ -1,11 +1,6 @@
-import { defineComponent, h, ref } from "vue";
+/** @entryState */
+const counter = { count: 0 };

-export default defineComponent({
-    setup() {
-        const count = ref(0);
-        async function onReset() {
-            count.value = 0;
-        }
-        return () => h("button", { onClick: onReset }, String(count.value));
-    }
-});
+export async function onReset() {
+    counter.count = 0;
+}
```

模拟器直接从 `onReset()` 开始。它**没有入口参数**：浏览器虽然传来一个事件，但处理器并没有读它。真正用到的事件数据才应该作为输入；`counter` 不是点按钮时传进来的值，所以不能改成 `onReset(counter)`。它是这次点击所属组件已有的状态，要写回的也是这份状态。

声明里的 `{ count: 0 }` 给出状态的形状，沿用了组件创建时的初值；分析一次点击时，**不把已有计数假定为 0，也不会在入口开始时重置它**。如果点击前界面显示 8，是处理器里的 `counter.count = 0` 这一步把它清成 0。

**真正属于 P 的输入，直接写在入口函数的参数列表里。** 比如页面启动一次加载时，宿主已经给这次动作选定了账户和计费日。截取这段业务入口，预处理把这两个值放进签名，不再让处理器从宿主对象里找它们：

```diff
--- a/src/ArrearsPage.vue
+++ b/src/ArrearsPage.ts
@@ -1,7 +1,5 @@
-const props = defineProps<{ accountId: number; today: number }>();
-
-async function onLoad() {
-    const todayAtEntry = props.today;
-    const bills = await loadBills(props.accountId);
+export async function onLoad(accountId: number, today: number) {
+    const todayAtEntry = today;
+    const bills = await loadBills(accountId);
     updateSummary(bills, todayAtEntry);
 }
```

**P 是本次调用的不可变输入。** 上例的 `accountId` 和 `today` 是进入时绑定的值，等完 `await` 也还是这两个值。下一次调用可以传新的值。对象型输入也一样：不能把仍会变化的宿主对象当成不可变入参。如果原程序在 `await` 之后会重新读取会变的 `props.today`，就必须保留那一步读取及它的时机，不能用入口参数把它冻结。这里展示的是入口函数签名应有的形状，不是在声明当前 Vue 适配器已经支持这种签名。

入口也不一定是动作。分析模板给金额卡片传了什么，就把绑定表达式提成一个无参函数。实例状态仍然用 `@entryState`，用 `@componentProp` 指明这是哪个组件实例的哪个属性。例如同一个金额卡片有两个绑定，提取后各有自己的入口：

```diff
diff --git a/src/ArrearsPage.vue b/src/ArrearsPage.mjs
rename from src/ArrearsPage.vue
rename to src/ArrearsPage.mjs
--- a/src/ArrearsPage.vue
+++ b/src/ArrearsPage.mjs
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
+/** @entryState */
+const arrears = {
   status: { ready: false, loading: false },
   summary: { payable: 0 }
-})
-</script>
+}
+
+/** @componentProp PayableCard#balance.amount */
+export function amount() {
+  return arrears.status.ready ? arrears.summary.payable : 0
+}
+
+/** @componentProp PayableCard#balance.caption */
+export function caption() {
+  return arrears.status.loading ? '加载中' : '应付金额'
+}
```

两种入口用的是同一个原则：**提取入口不等于重新设计接口。** 点击是一次处理器调用，模板绑定是一次求值；都用原来那个实例的状态，不给函数凭空加一个状态参数。

### Java 实例状态：标记来源，不改变读写

下面的 `quota` 和 `this.quota` 同名、同类型，来源却不同：前者是调用者这次传来的新额度，后者是服务实例已有的额度。预处理只在实例字段的声明上加 `@EntryState`，其余照旧：

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

模拟器还是按 Java 的名字绑定规则读这两个值。也不需要把 `this.quota.units` 拆成参数，或把 `Quota q = this.quota` 产生的别名再登记成一份状态。如果读写真的用到了同一个对象，预处理得把这层关系保住。类似地，`@Nullable` 只在有依据时表达可空合同，不能为通过解析临时把值假定为非空。

## 二、再判断调用该在哪一层停下

入口和状态写清楚后，沿原程序往下走。遇到看不到实现的调用，先分清它是哪一种：有明确含义的操作可以直接交给模拟器；能用源码还原的可以补模型；只有真实的外部调用才标记为外部边界。

### 认出原生操作，就停在这一层

判断一段字符串是不是空白，可以有很多种写法。一个库逐个检查字符，另一个库先查字符类别。库可以有很多套，但如果两种写法判断的是同一件事，模拟器只需要一种操作：判断空白。

这里说的**原生操作**，是模拟器知道怎么算的操作。一旦认出它在判断整段文本是否空白，就停在字符串这一层，不必进库里重放每次字符读取，更不用追到字节数组。但该保留的区别不能省：两个库对 `null` 或某个字符是否算空白的处理不同，就不能当成同一个操作。如果原程序观察的就是编码后的字节，那些字节也不能省掉。

比如下面这段代码先裁剪字符串，结果空白时再用备用文本。改写只换两个调用的目标，并把原本允许 `null` 的参数明确标出来。

```diff
--- a/src/LabelService.java
+++ b/src/LabelService.java
@@ -1,7 +1,7 @@
-import org.apache.commons.lang3.StringUtils;
+import normalizer.intrinsics.NormalizedText;

 class LabelService {
-    String normalizeLabel(String raw, String fallback) {
-        return StringUtils.defaultIfBlank(StringUtils.trim(raw), fallback);
+    String normalizeLabel(@normalizer.annotations.Nullable String raw, String fallback) {
+        return NormalizedText.defaultIfBlank(NormalizedText.trim(raw), fallback);
     }
 }
```

调用的顺序没变：先裁剪，再检查是否空白。归到原生操作之后，最大的好处是**数学可以接着算**。假设已经确认 `func1` 和 `func2` 都是在判断同一个字符串是否非空白，结果相同，也没有副作用或额外异常。那就不用知道 `x` 是什么：

```text
func1(x) == func2(x)
  → isNotBlank(x) == isNotBlank(x)
  → true
```

操作自己也带着可用的规律。比如按这里的定义，`isNotBlank(null)` 是假。所以在下面的判断里，只要右边是真的，左边就一定是假的：

```text
isNotBlank(x) AND x == null
  → isNotBlank(null) AND x == null
  → false
```

如果这是一个 `if`，真分支根本走不到。模拟器没打开字符数组，也没猜 `x` 的内容，只用了这个操作本身的性质。

### 没源码的 DTO：用常识补出最小模型

有时拿不到 JAR 里的 Java 源码，但看得出一个类只是用来传数据的 DTO。比如公开接口有成对的 `int getAmount()` 和 `void setAmount(int amount)`，调用处也把它当作金额字段来读写。如果类的用途、方法签名和已有用法都符合这个惯例，AI 可以先按普通 getter/setter 来建模：读就读字段，写就写字段。**不必因为缺了源码就连简单的对象读写都停下来。**

下面是建议补入验收的写法。例如，原程序对已有的 `AmountDto` 调用 `setAmount(n)`，接着调用 `getAmount()`。调用处不改，AI 为模拟器补出这几行：

```diff
--- /dev/null
+++ b/src/vendor/billing/AmountDto.java
@@ -0,0 +1,13 @@
+package vendor.billing;
+
+public class AmountDto {
+    private int amount;
+
+    public int getAmount() {
+        return this.amount;
+    }
+
+    public void setAmount(int amount) {
+        this.amount = amount;
+    }
+}
```

模拟器照常执行：先在同一个对象上把 `amount` 写成 `n`，再读回 `n`。这是**只用于分析的模型**，不是去改原来的 JAR。它也没说传进来的已有对象的金额一开始是 0；如果没写过就读，读的是那个对象原有的值。

这是一条**要写明的假设**，不能只凭 `get`、`set` 两个前缀就套用。有文档、测试或字节码时，可以拿来核对。只要发现 setter 会校验、舍入或写别的字段，或者 getter 会额外计算，就不能继续假设它只是字段读写；要按查到的行为建模，弄不清就停。

### 外部调用：知道发给谁，不知道对方回什么

**原生操作，模拟器知道怎么算；外部调用，模拟器知道调用发生了，但不知道对方会回什么。** `@External` 画的是这条线。

例如，程序找外部服务取风险分数。AI 在客户端字段上加标记，不改方法里的调用和异常处理：

```diff
--- a/src/RiskService.java
+++ b/src/RiskService.java
@@ -1,15 +1,18 @@
+import normalizer.annotations.External;
+
 class RiskService {
+    @External
     private final RiskClient riskClient;

     RiskService(RiskClient riskClient) {
         this.riskClient = riskClient;
     }

     int riskScore(int accountId) {
         try {
             return riskClient.fetchScore(accountId);
         } catch (RiskUnavailable ignored) {
             return 0;
         }
     }
 }
```

模拟器走到 `fetchScore(accountId)`，不会真的发请求，也不会编一个分数。它知道这次调用带了哪个账户号。成功时，用一个符号代表**这次调用返回的分数**；如果抛出 `RiskUnavailable`，就按原代码走进 `catch`，返回 0。这个 0 只属于失败那条路；成功的分数也不能改成让入口调用者提前传进来。

只有确认这本来就是一次外部调用，才标 `@External`（JS/TS 的写法是 `/** @external */`）。没找到源码，不等于它是外部服务。调用两次，不代表拿到同一个结果；对一次回执读两遍，也不能变成两次请求。回调同理：**登记不等于执行**。什么时候真正调用回调，还是由原程序决定。

## 三、什么时候才需要替换一段计算

入口和调用边界都清楚了，通常就可以把原程序交给模拟器。还有一种情况：代码完全可见，但如果只按原样逐步执行，拿不到完整的符号结果。先看一个不该改的例子，再看真正需要补语义的情况。

### 反例：不要把 `switch` 改写成 `if`

下面这个方法只有普通 Java 分支。即便把它改成 `if` 能得到相同结果，这也不是 AI 该做的预处理：

```diff
--- a/src/SwitchMode.java
+++ b/src/SwitchMode.java
@@ -1,12 +1,7 @@
 class SwitchMode {
     static int classify(int code) {
-        switch (code) {
-            case 1:
-                return 10;
-            case 2:
-                return 20;
-            default:
-                return 0;
-        }
+        if (code == 1) return 10;
+        if (code == 2) return 20;
+        return 0;
     }
 }
```

**正确交付：保留原来的 `switch`。** 即使不知道 `code` 的值，模拟器也能分别考虑命中各个 `case` 和走 `default` 的情况；入口参数未知，不妨碍模拟器按 Java 规则执行。当前解析器如果不接受 `switch`，该补的是工程支持，不是让 AI 换一种写法绕过去。

### 未知次数的循环：补的是等价计算，不是另一种写法

和 `switch` 不同，循环即使每一步都能执行，也可能因为次数取决于未知输入、又没有可用的固定上限，没法逐轮走完。这时才需要考虑一条有依据的等价计算规则。

例如，下面的循环从 0 开始，每次加一，直到到达 `limit`。模拟器还不知道 `limit`，但知道循环数的是从 0 到 `limit` 之前的整数。AI 只把循环体所在的那一段换成模拟器认识的计数操作：

```diff
--- a/src/VariableLoop.java
+++ b/src/VariableLoop.java
@@ -1,9 +1,9 @@
 class VariableLoop {
     static int count(int limit) {
         int total = 0;
-        for (int i = 0; i < limit; i++) {
-            total = total + 1;
-        }
+        /* @normalizer-ir
+           total = count from [0, limit) as i
+        */
         return total;
     }
 }
```

如果 `limit = 3`，就数 0、1、2，结果是 3；如果 `limit <= 0`，区间为空，结果还是 0。它交的是「从哪里数到哪里」，不是预先算好的次数。要是原循环除了计数还会发请求或改状态，就不能只留下计数。

固定上限重试则不必照着这个例子改。原代码明确写了最多尝试三次，并且成功就返回，保留循环和异常捕获即可。`continue` 和 `break` 本身也是 Java 语法，应由模拟器理解。只有确实要把整个循环交成一条聚合或折叠规则时，才在这条规则里保留原来的跳过和停止时机；不能用一个筛选条件代替它们。

### 把查询和循环接起来：SQL 与局部聚合

Mapper 的 Java 方法可能没有方法体，但注解里有 SQL。这和看不见远端服务怎么处理请求不是一回事。以「汇总本账户未结账单的正金额」为例：查询按账户和状态取行，Java 循环再筛出正金额相加。预处理在**原查询位置**交出取行的含义，在原循环位置交出求和的含义，不把查询结果变成一个新入参。

```diff
--- a/src/ArrearsService.java
+++ b/src/ArrearsService.java
@@ -1,36 +1,24 @@
 import java.util.List;
-import org.apache.ibatis.annotations.Select;

 class ArrearsService {
-    private final BillMapper billMapper;
-
-    ArrearsService(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
     int positiveTotal(int accountId) {
         int total = 0;

-        for (BillRow row : billMapper.selectOpenBills(accountId)) {
-            if (row.amount > 0) {
-                total += row.amount;
-            }
-        }
+        List<BillRow> rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId and b.status == "OPEN"
+               yield { accountId: b.account_id, amount: b.amount, status: b.status }
+        */
+
+        /* @normalizer-ir
+           total += sum from rows as row
+               where row.amount > 0
+               yield row.amount
+        */

         return total;
     }
-}
-
-interface BillMapper {
-    @Select("""
-        SELECT account_id AS accountId,
-               amount,
-               status
-        FROM bill
-        WHERE account_id = #{accountId}
-          AND status = 'OPEN'
-    """)
-    List<BillRow> selectOpenBills(int accountId);
 }

 class BillRow {
```

`@normalizer-ir` 是给模拟器读的局部语义，不是让 JVM 执行注释。这里故意保留两个步骤和原有的 `total = 0`、`return total`；把两处筛选合起来，是模拟器的工作。遇到首行选择，要保留决定「首行」的排序；遇到更新或删除，要交出目标表、条件以及原有的行数和失败合同。把写入只标成一个黑盒方法，会丢掉它实际改了什么。如果查询本来在另一次调用之后，也不能为了方便把它移到入口。

## 四、证据不足，就指出哪里还不能算

有些循环不能写成计数，也不能证明有限。例如：

```java
while (!client.isReady(accountId)) {
    client.waitForChange(accountId);
}
return client.readScore(accountId);
```

这个程序不知道何时会等到就绪，也没给最多等几次。就算模拟器知道这两个调用都发到哪里，也不能因此知道循环会在什么时候结束。AI 不能自作主张改成「试三次就算了」，也不能跳过等待直接读分数。源码和约定都给不出结束条件或等价做法时，就明确说：**这段还不能执行到结果，缺的是可确认的等待边界。**

同样，只看见接口却无法确定这次调用到哪个实现，或者模拟器没有足够的合同解释原操作，都应指出具体位置和缺的条件（`PREPROCESS_REQUIRED`），不造一个默认值，也不把一份部分结果当成完整结果。

圈地的结果应该让人看得出来：**哪些地方模拟器自己算，哪些地方按说明补模型，哪些地方等外部给值，哪些地方证据不足必须停。**
