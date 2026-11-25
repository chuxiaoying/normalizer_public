# PERS 符号定义与源码绑定

本文定义面向使用者的 PERS 公共 YAML 形态。目标是让 PERS 中的每个 P/E 符号都有明确来源，同时让下游开发者只使用通用 YAML 解析能力就能读取结果。

PERS 是一个 YAML 文档。P、E 是来源定义区，S 是有序操作序列，R 是正常完成结果，throws 是异常完成集合。公式和条件保持表达式形式，由 YAML 字段承载，不再额外设计一套 PERS 表达式 AST。

## 顶层结构

~~~yaml
schema: normalizer.pers/1

entry: <本次归一化入口>

P:
  parameters:
    <参数名>: <入口参数位置或对象定义>
  props:
    <prop 名称>: <内核类型>

E:
  <关系根或状态根>:
    <来源类型>: <来源细节>
    fields:
      <字段名>: <内核类型或嵌套字段定义>
    collections: <集合成员定义>
    <数据库表名>:
      <数据库列名>: <内核类型>

S: <按源码顺序排列的操作节点>

R: <结果表达式>

throws: <按规范排列的异常完成记录>
~~~

P 和 E 的每个名称都必须在自己的来源分组中显式出现。`parameters`、`props` 和 `fields` 是来源分组，数据库表名本身就是关系来源命名空间，不再需要额外的 `table` 或 `columns` 包装。`fields` 是递归成员树，叶子的值是内核类型，带子项的字段自然表示记录类型。分组中的每个映射键都是真实声明，不能靠入口、位置顺序或读者推断遗漏的名称。

每条 `throws` 记录有异常 `type`，可附 `when` 和 `payload` 表达式。类型相同的不同载荷可以分别发布，条件保留实际首匹配域。只观察类型时省略 `payload`；已观察到空值用表达式字符串 `payload: 'null'`，不能将字段省略或写成 YAML null。`R` 只覆盖正常完成。异常排列规则见[统一正则规范](../../design/unified-canonical-form.md#顶层异常结果的稳定排列)。

公共来源放在父级，局部来源放在子级。这样可以避免每个参数重复入口签名，也可以把 SQL 表和列、VO 状态根和字段组织成明确的嵌套关系。

## 来源分组的最小写法

来源类别只在能够区分真实语义的层级出现一次。一个名称旁边如果只能写同一个固定标记，就把这个标记提升为父级分组，同时仍然逐项列出所有名称：

| 来源 | 公共写法 | 仍然保留的信息 |
|---|---|---|
| Java 入口参数 | `P.parameters.value: { position: 0, type: number }` | 参数名、入口位置和内核类型 |
| Vue props | `P.props: { accountId: number, today: date }` | 每个 prop 名称和内核类型 |
| 对象字段 | `fields: { loading: bool, ready: bool }` | 每个字段名称、内核类型和嵌套关系 |
| SQL 表列 | `E.bill: { account_id: number, amount: money }` | 数据库表命名空间、物理列名和内核类型 |

这种压缩只删除 `value: parameter`、`accountId: prop` 或 `loading: field` 这类没有额外信息的重复标记。它不把名称变成隐式来源，也不允许通过参数位置、源码上下文或顺序猜测未声明的 P/E。SQL 列直接使用数据库物理名称，因此不需要在 PERS 中再维护一套别名映射。AI 预处理负责把源码中的 SQL 别名消化为这个规范名称。

## 叶子的内核类型

P/E 叶子右侧使用统一内核的业务类型名，不使用 Java、TypeScript 或 SQL 的宿主实现类型名。当前公共标量类型包括 `number`、`money`、`date`、`bool` 和 `text`，可空值在需要时使用对应的 `?` 形式。记录类型不单独写 `type: record`，它由带子项的 `fields` 映射直接表达。

例如，TypeScript 的 `number` 需要由 AI 预处理根据业务合同收束为 `number` 或 `money`，日期语义收束为 `date`。欠费单里的账户编号使用 `number`，金额使用 `money`，状态标志使用 `bool`。这个选择属于进入统一 IR 前的类型确定，不能让 PERS 暴露 Java 的 `int`、TypeScript 的实现宽度或 SQL 的具体数值类型。

有序只读文本 Map 的出口声明为 `map<text,text>`，表达式表示为 `MAP([["key", value], ...])`；空 Map 为 `MAP([])`。每个键只出现一次，列出的顺序就是值的迭代顺序。一个没插入的键不占槽位，不能把它改成 null 值或按键名重排。当前[正式示例](../../design/examples/topics/language-entries/javascript-module.md#有序只读-map可空输入只在非空时插入)仅认证已取得的可空文本条件构造，不宣布通用 Map 输入或可变 Map 语义。

参数同时需要来源位置和内核类型时使用对象描述：

~~~yaml
P:
  parameters:
    accountId:
      position: 0
      type: number
~~~

`position` 解决入口绑定，`type` 解决内核类型。对象参数或对象 E 通过嵌套 `fields` 给出记录成员类型，不重复添加 `type: record`。

## P 的显式声明

Java 函数的公共签名声明一次，每个 P 仍然单独声明自己的来源：

~~~yaml
entry:
  function: src/HelloWorld.java#HelloWorld.add
  signature:
    parameters: [number, number]
    returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    delta:
      position: 1
      type: number
~~~

value 和 delta 是两个独立的 P 定义。`position` 把它们绑定到入口签名中的参数位置，`type` 给出内核看到的参数类型，避免依赖局部变量命名或上下文猜测。

对象参数沿用同一结构。对象根声明一次，字段在子级逐项声明：

~~~yaml
P:
  parameters:
    request:
      position: 0
      fields:
        value: number
        limit: number
        fallback: number
~~~

request 是一个记录型 P。`fields` 的嵌套结构给出记录成员和它们的内核类型，因此不再额外写一个重复的 `type: record`。request.value、request.limit 和 request.fallback 是它的字段引用。只有 PERS 名称和来源字段名不一致时，才使用显式别名映射，例如 `aliases: { displayLimit: limit }`。

Vue 组件的 P 是 props，结构保持一致：

~~~yaml
entry:
  component: src/ArrearsPage.vue#ArrearsPage
  event: onLoad

P:
  props:
    accountId: number
~~~

组件自身的 VO 不属于 P。它是入口已经存在的对象状态，进入 E。

## E 的嵌套状态定义

~~~yaml
E:
  vo:
    state: src/ArrearsPage.vue#ArrearsPage.vo
    fields:
      loading: bool
      ready: bool
      query:
        accountId: number
~~~

这个定义同时建立了：

~~~text
vo                  → 页面 VO 状态根
vo.query            → VO 的 query 字段
vo.query.accountId  → query 内的 accountId 字段
vo.loading          → loading 字段
vo.ready            → ready 字段
~~~

字段引用使用稳定路径，例如 vo.query.accountId。`fields` 是一棵递归成员树，`loading: bool` 表示一个布尔叶子字段，`query: { accountId: number }` 表示 `query` 是一个记录字段并继续声明其成员。叶子右侧的类型来自统一内核合同，不能把宿主语言的类型声明原样当作 PERS 来源定义。来源字段和 PERS 名称不一致时才展开显式映射，例如 `aliases: { displayLoading: loading }`。代码中出现的同名对象不会自动进入这个命名空间。

state 表示组件或业务对象已经持有的可变状态根。它的值直接指向状态对象的正式身份：

~~~yaml
E:
  vo:
    state: src/ArrearsPage.vue#ArrearsPage.vo
    fields:
      loading: bool
      ready: bool
~~~

vo 是对象 E 的 PERS 引用名，state 是来源类别，路径中的 vo 是源码状态对象名。`fields` 同时表达状态根的直接字段和对象字段，叶子类型表达字段的内核类型，嵌套映射表达记录成员。对象 E 由“状态根加递归字段树”完整表达，不再增加重复的 object 分组。props、参数、SQL 关系和 external 完成值分别使用各自的来源声明，不能把它们归入 state。

### 对象 E 的完整形态

对象成员不需要单独的 `objects` 分组。一个字段的值是否为对象，直接由 `fields` 的形状表达：类型值是叶子字段，嵌套映射是继续展开的记录字段。这样字段名称、对象层级、内核类型和源码路径保持同一棵树。

#### 扁平状态对象

源码状态：

```ts
const profile = reactive({
  displayName: "",
  level: 0,
})
```

PERS 来源定义：

```yaml
E:
  profile:
    state: src/ProfilePage.vue#ProfilePage.profile
    fields:
      displayName: text
      level: number
```

引用路径是 `profile.displayName` 和 `profile.level`。两个字段都是 `profile` 的叶子成员。

#### 状态根包含多个对象成员

源码状态：

```ts
const arrears = reactive({
  status: { loading: false, ready: false, failed: false },
  summary: { payable: 0 },
})
```

PERS 来源定义：

```yaml
E:
  arrears:
    state: src/ArrearsPage.vue#ArrearsPage.arrears
    fields:
      loadedAccountId: number
      status:
        loading: bool
        ready: bool
        failed: bool
      summary:
        payable: money
```

引用路径是 `arrears.loadedAccountId`、`arrears.status.loading`、`arrears.status.ready`、`arrears.status.failed` 和 `arrears.summary.payable`。`loadedAccountId` 是叶子字段，`status` 与 `summary` 是继续展开的对象字段。

#### 同时包含直接字段和嵌套对象

源码状态：

```ts
const order = reactive({
  orderId: "",
  total: 0,
  status: { code: "OPEN", label: "Open" },
  customer: { id: "", name: "" },
})
```

PERS 来源定义：

```yaml
E:
  order:
    state: src/OrderPage.vue#OrderPage.order
    fields:
      orderId: text
      total: money
      status:
        code: text
        label: text
      customer:
        id: text
        name: text
```

`orderId` 和 `total` 是 `order` 的直接叶子字段，`status` 和 `customer` 是对象字段，`order.status.code` 和 `order.customer.name` 是递归字段路径。它们全部属于同一个 `fields` 树。

#### 多层嵌套对象

源码状态：

```ts
const shipment = reactive({
  recipient: {
    identity: { id: "", displayName: "" },
    address: { city: "", detail: "" },
  },
})
```

PERS 来源定义：

```yaml
E:
  shipment:
    state: src/OrderPage.vue#OrderPage.shipment
    fields:
      recipient:
        identity:
          id: text
          displayName: text
        address:
          city: text
          detail: text
```

引用路径是 `shipment.recipient.identity.id` 和 `shipment.recipient.address.city`。对象层级来自 YAML 的嵌套层级，不需要为每个节点再写 `object` 或 `field` 固定标记。

#### 同名字段位于不同对象

源码状态：

```ts
const comparison = reactive({
  current: { status: "OPEN", amount: 10 },
  previous: { status: "CLOSED", amount: 8 },
})
```

PERS 来源定义：

```yaml
E:
  comparison:
    state: src/ReportPage.vue#ReportPage.comparison
    fields:
      current:
        status: text
        amount: money
      previous:
        status: text
        amount: money
```

`comparison.current.status` 和 `comparison.previous.status` 是两个不同的 E 引用。对象路径已经完成消歧，不需要给字段强行增加编号或临时别名。

#### 对象字段存在来源别名

源码状态：

```ts
const customer = reactive({
  id: "",
  displayName: "",
})
```

PERS 来源定义：

```yaml
E:
  customer:
    state: src/CustomerPage.vue#CustomerPage.customer
    fields:
      id: text
      displayName: text
    aliases:
      displayName: display_name
```

`customer.id` 直接对应源码字段 `id`。`customer.displayName` 通过显式别名对应来源字段 `display_name`。别名只在源码名称确实不同的时候出现，不能用它替代同名字段列表。

#### 集合成员的对象元素

源码状态：

```ts
const invoice = reactive({
  lines: [{ sku: "", amount: 0, discount: { rate: 0 } }],
})
```

PERS 来源定义：

```yaml
E:
  invoice:
    state: src/InvoicePage.vue#InvoicePage.invoice
    collections:
      lines:
        element:
          fields:
            sku: text
            amount: money
            discount:
              rate: number
```

引用路径是 `invoice.lines[*].sku`、`invoice.lines[*].amount` 和 `invoice.lines[*].discount.rate`。`collections` 表明成员是集合，`element` 表明下面定义的是每个元素的对象结构。集合元素字段不能直接提升为 `invoice.sku`。

这些形态都遵循同一个递归规则：状态根用 `state` 绑定源码对象身份，所有对象成员和直接字段都在同一棵 `fields` 树中，叶子值使用内核类型，集合成员用 `collections`，真实名称差异用 `aliases`。每个引用路径都能沿 YAML 的键逐段回到源码对象，不依赖对象名称的默契。

## E 的嵌套 SQL 关系定义

表名和列名属于同一个关系定义：

~~~yaml
E:
  bill:
    account_id: number
    due_date: date
    amount: money
    waived_amount: money
    priority: number
    status: text
~~~

关系公式引用 bill.account_id、bill.status 和 bill.amount。公式不需要重复写物理表名和物理列名：

~~~yaml
R: SUM(bill.amount - bill.waived_amount FROM bill)
~~~

表来源、列来源和 PERS 引用路径在一个嵌套 E 定义中闭合。表身份使用数据库的表命名空间，表名不再重复写成 `table: bill`。同名列直接写业务类型，公共形态默认把物理列名作为规范引用名。同一张表在关系中出现多次时，使用不同的 PERS 关系根或表达式别名区分。

## 其他 E 来源

已有集合、缓存快照和纯环境快照也必须有明确来源：

~~~yaml
E:
  cache:
    collection:
      owner: src/LocalCache.java#LocalCacheService.snapshot
      name: entries

  clock:
    environment:
      provider: system.clock
      name: today
      type: date
~~~

如果集合或环境值需要通过调用才能得到，它属于 S 的 external 结果。S 产生的 $e_n 不在 E 中重复声明。

## YAML 承载表达式

YAML 负责承载 P/E 定义、S 序列以及 R、when、value、args 等字段。字段内部保留业务表达式：

~~~yaml
R: value + 1

S:
  - write:
      target: vo.loading
      value: true
      when: completion.is_return($e_0)
~~~

多行关系公式使用 YAML block scalar，公式本身仍然是表达式：

~~~yaml
R: |-
  CASE
    WHEN total > quota THEN quota
    ELSE total
  END
~~~

下游需要通用 YAML 解析器和既有表达式能力。PERS 不再增加一套把 add、case 或 aggregate 拆成 YAML 节点的专用语法。

## S 的顺序和完成值绑定

S 是 YAML 序列，序列顺序就是副作用顺序：

~~~yaml
S:
  - write:
      target: vo.loading
      value: true

  - call:
      external: profileClient.fetchProfile
      args:
        - accountId
      result: $e_0

  - write:
      target: vo.loading
      value: false
      when: completion.is_return($e_0)
~~~

每个 call 都必须在 S 中显式写出结果绑定。$e_0、$e_1 是完成值的公共引用名，调用顺序只表达时序，不负责偷偷改变引用名。结果不需要再写一份 E：

~~~yaml
S:
  - call:
      external: profileClient.fetchProfile
      args:
        - accountId
      result: $e_0
  - call:
      external: arrearsClient.queryOpenBills
      args:
        - $e_0.accountId
      result: $e_1
~~~

S 中的后续节点只能引用 P、E 或此前已经显式绑定的 $e_n。状态写入本身不制造新的 E。

## 引用限制

合法结果：

~~~yaml
P:
  parameters:
    limit:
      position: 0
      type: number

E:
  policy:
    state: Pricing.policy
    fields:
      limit: number

R: |-
  CASE
    WHEN limit > policy.limit THEN limit
    ELSE policy.limit
  END
~~~

非法结果：

~~~yaml
R: abc + 1
~~~

abc 没有 P 或 E 定义。即使源码里存在名为 abc 的局部变量，也不能凭上下文猜测其来源。

关系根同样必须先定义：

~~~yaml
R: SUM(unknownBill.amount FROM unknownBill)
~~~

如果 unknownBill 没有 E 定义，结果应被拒绝。

## PERS 与源码的关系

~~~text
Java 参数 accountId      → P.parameters.accountId
Vue prop accountId       → P.props[accountId]
VO 字段 vo.loading       → E.vo.fields.loading
SQL 表 bill              → E.bill
SQL 列 account_id        → E.bill.account_id
external 完成值          → S 中第 n 个 call 产生 $e_n
~~~

这份定义关系属于 PERS 本身。它不需要另一份 mapping YAML，也不依赖源码局部变量的默契。
