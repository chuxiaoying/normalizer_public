# PERS登场

看一段创建订单的程序：调用方说「要 3 件」，程序读到当前还剩 10 件，先把剩余量写成 7，再请求外部服务创单，最后返回订单号。只写「返回订单号」，就漏掉了扣减和创单；只写「扣了 3 件」，又说不清订单号从哪里来、失败时已经做了什么。

**PERS 是一次符号执行的完整业务结果。** 参数、环境、返回值和副作用都要交代。内核不实际创单，也不需要知道订单号是多少；它算的是「给这些输入、碰到这些环境和回执，程序会怎么结束、会做哪些事」。

| | 含义 | 这个例子里是什么 |
| --- | --- | --- |
| **P · Parameters** | 调用方给这一次入口的输入 | 本次要下单的 `units` |
| **E · Environment** | 程序从外界获得的信息 | 进场时的 `remaining`；创单回执也是外界给的值，但绑定在那次调用上 |
| **R · Response** | 入口正常结束时交给调用方的值 | 这一次创单返回的订单号 |
| **S · Side effects** | 执行中发生的可观察动作，按发生顺序记录 | 先写剩余量，再发起创单 |

程序也可能抛异常。异常完成单独列出，不混进正常的 R，也不会抹掉异常之前已经发生的 S。

为便于阅读，下文的 YAML 示例省略了机器导出中的格式版本字段 `schema: normalizer.pers/1`；业务字段照常展示。

## 例一 · Java 简单入口：只有 P 和 R

**为什么是这个入口？** `run` 是 `src/Add.java` 里的静态方法，不需要接收对象，所以入口写成 `src/Add.java#Add.run`。代码只做加法，没有读取已有状态，也没有对外动作。

```java
class Add {
    static int run(int value, int delta) {
        return value + delta;
    }
}
```

```yaml
entry:
  function: src/Add.java#Add.run
  returns: number
P:
  parameters:
    value: { position: 0, type: number }
    delta: { position: 1, type: number }
R: delta + value
```

这份 YAML 定义了最基本的字段：

| 字段 | 在这个例子里的含义 |
| --- | --- |
| `entry`、`entry.function` | `entry` 描述本次分析的入口，`function` 统一写成「工程相对路径#入口调用」，指向可实际发起的调用。 |
| `entry.returns` | 正常返回的业务类型。这里是 `number`；它不是返回值本身。 |
| `P`、`P.parameters` | `P` 声明调用方提供的输入。`parameters` 按参数名列出定义，这里是 `value` 和 `delta`。 |
| `position`、`type` | 每个参数在原签名里的位置和业务类型。位置从 0 开始：`value` 是第一项，`delta` 是第二项。 |
| `R` | 正常返回值的表达式。传入 2 和 3 得 5；分析未知入参时，按统一写法交付 `delta + value`，参数位置不变。 |

本例没有环境来源、对外动作或异常完成，不写 `E`、`S` 或 `throws`。

Java 和 TypeScript 都用 **`工程相对路径#入口调用`**。路径指向声明调用起点的文件；`#` 后写从那里怎么调用，不写某次调用的参数值。Java 可以从静态方法开始，也可以经静态字段取得对象再调用实例方法；TypeScript 从模块导出的函数开始。这里的「静态」是调用起点能从源码定位，不要求最后调用的方法声明为 `static`。

有歧义时只补必要的信息：例如重载用 `src/Add.java#Add.run(int, int)` 区分。PERS 里的 `number` 是业务类型，不能用来区分 Java 的 `int` 和 `long` 重载。状态来源和外部端口也统一用「工程相对路径#绑定路径」定位。入口指向怎么发起调用，状态来源指向哪份已有状态，外部端口指向调用所绑定的接收者和方法。

## 例二 · Java 完整入口：从已有状态到外部回执

**为什么是这个入口？** 分析的是已有实例上的 `place` 方法，所以用 **`Beans.DISPATCH.place(units)`** 定位接收对象。从类名 `Beans` 出发，取静态字段 `DISPATCH`，再调用实例方法 `place`。静态初始化代码显式创建创单端口和 `Dispatch` 实例；分析的是这个已装配实例上的一次调用，不重新构造对象。启动时写的 10 不等于每次调用前都还剩 10，`@N.EntryState` 标记的是当次已有状态。

原工程的 `OrderPort.create` 是生产环境绑定到订单服务的网关。它的声明给出参数、正常回执和已知失败。规范源码用 `@N.External` 标记接收者字段。内核执行到调用处时记录外部动作并取得符号回执，不调用真实网关，也不把端口变成入口参数。本例只讨论扣减与发起调用的顺序，不额外假造库存校验或失败补偿。

```java
import normalizer.intrinsics.N;

class Beans {
    static final Dispatch DISPATCH = new Dispatch(new OrderPort(), 10);
}

class Dispatch {
    @N.EntryState
    private int remaining;
    @N.External
    private final OrderPort orders;

    Dispatch(OrderPort orders, int initialRemaining) {
        this.orders = orders;
        this.remaining = initialRemaining;
    }

    int place(int units) {
        remaining = remaining - units;
        return orders.create(units, remaining);
    }
}

class OrderPort {
    native int create(int units, int remaining) throws OrderUnavailable;
}

class OrderUnavailable extends RuntimeException {}
```

这次执行起步时，参数 `units` 和旧剩余量都还未知。执行第一句时，内核从已有状态取旧值，算出 `remaining - units`，写回同一字段。执行第二句时，参数里第二项读取的是**刚写过的值**；外部服务给这次调用的未知回执记为 `$e_0`。下面是这条执行的 PERS。YAML 写 `entry.function: src/Dispatch.java#Beans.DISPATCH.place`，因为从同一个文件里的 `Beans.DISPATCH` 取得接收对象以后，才能调用 `place`。只写 `Dispatch.place` 会丢掉这条实例取得路径。

```yaml
entry:
  function: src/Dispatch.java#Beans.DISPATCH.place
  returns: number

P:
  parameters:
    units: { position: 0, type: number }

E:
  remaining:
    kind: state
    source: src/Dispatch.java#Beans.DISPATCH.remaining
    type: number

S:
  - write:
      target: remaining
      value: remaining - units
  - call:
      external: src/Dispatch.java#Beans.DISPATCH.orders.create
      args: [units, remaining - units]
      returns: number
      result: $e_0

R: $e_0

throws:
  - type: OrderUnavailable
    when: completion.is_throw($e_0, OrderUnavailable)
```

这一份在 P 和 R 之外增加了环境、动作和异常完成：

| 字段 | 在这个例子里的含义 |
| --- | --- |
| `E`、`E.remaining` | `E` 声明环境来源；`remaining` 是给入口已有库存取的引用名，对应初值，不是额外入参。 |
| `kind`、`source`、`type` | `kind: state` 表示已有状态，`source: src/Dispatch.java#Beans.DISPATCH.remaining` 指到哪个实例上的字段，`type: number` 说明这个标量状态的业务类型。 |
| `S` | 有序的可观察动作序列。本例先写库存，再调用外部服务。 |
| `write`、`write.target`、`write.value` | 一次写入、写到哪、在该动作发生时写入什么。这里把 `remaining - units` 写回 `remaining`。 |
| `call`、`call.external`、`call.args` | 一次外部调用、调哪个端口的方法、按原顺序传什么实参。`external` 用和状态来源相同的定位规则；`args` 的第二项是扣减后的数值。 |
| `call.returns`、`call.result` | 前者是外部调用成功时的回执类型，后者为**本次**调用的完成绑定名字 `$e_0`。成功时 R 读它的值；失败时按其完成状态判断异常。 |
| `throws`、`throws[].type`、`throws[].when` | 入口可能抛出的异常清单、异常类型以及发生条件。这里是当 `$e_0` 以 `OrderUnavailable` 失败时，入口也抛出该异常。 |

代入 `units = 3` 和旧值 `remaining = 10`：先写成 7，外部调用拿到的是 `create(3, 7)`。如果这次回执是订单号 42，入口就返回 42；如果调用以 `OrderUnavailable` 结束，入口就抛它，但先前写下的 7 还在。调用失败也不能证明服务端没创出订单；是否有提交、回滚或幂等保证，要看真实合同，不由 Normalizer 猜。

## 例三 · 前端生产：事件把新值写进页面状态

**为什么是这个入口？** 这一例分析数量变更事件，所以选 `src/OrderView.ts` 导出的 `onUnitsChanged`。写成 `src/OrderView.ts#onUnitsChanged`，能和别的模块里同名的处理器区分开。同一模块也导出下一例的属性入口。两个入口都绑定到同一个页面实例；下面的初始化只在创建实例时执行，不会在每次事件或渲染前重置。

```typescript
import * as N from "@normalizer/stdlib";

/** @N entryState */
const draft = { units: 0, ready: false };

export function onUnitsChanged(units: number): void {
    draft.units = units;
    draft.ready = true;
}

/** @N componentProp OrderCard#summary.units */
export function cardUnits(): number {
    return draft.ready ? draft.units : 0;
}
```

事件入口这次收到的数量属于 P，`draft` 是 E 里的已有状态。事件不返回业务值，但先写数量、再把状态标成就绪，两次写入都属于 S。

```yaml
entry:
  function: src/OrderView.ts#onUnitsChanged
  returns: void
P:
  parameters:
    units: { position: 0, type: number }
E:
  draft:
    kind: state
    source: src/OrderView.ts#draft
    fields: { units: number, ready: bool }
S:
  - write: { target: draft.units, value: units }
  - write: { target: draft.ready, value: true }
R: void
```

这一例新出现的是记录型状态：`E.draft` 是状态根，`source` 指向归一化模块里的 `draft`；`fields` 按字段名列出它的成员和业务类型，所以 `units` 是 `number`，`ready` 是 `bool`。它和上一例的标量不同，不在状态根再写一个 `type`。参数表里的 `units` 是本次输入，状态里的 `draft.units` 是写入目标，不能因为同名而合并。

传入 3，效果是先写 `draft.units = 3`，再写 `draft.ready = true`。**R 是 `void`，不等于没有业务结果。** 这里成果是页面状态的变化。

## 例四 · 前端消费：属性入口只读已有状态

**为什么是这个入口？** 要分析的是原模板里的 `units` 属性，不是整次渲染，也不是数量变更事件。准备好的模块导出 `cardUnits()`，所以实际入口是 `src/OrderView.ts#cardUnits`。源码上的 `@N componentProp OrderCard#summary.units` 另外说明它对应哪个组件、实例和属性。模板身份不代替实际可调用的入口。

```yaml
entry:
  function: src/OrderView.ts#cardUnits
  returns: number
P:
  parameters: {}
E:
  draft:
    kind: state
    source: src/OrderView.ts#draft
    fields: { units: number, ready: bool }
R: |-
  CASE WHEN draft.ready THEN draft.units ELSE 0 END
```

这次属性求值没有形参，`P.parameters: {}` 显式表示空参数表；不写状态，也不发请求，所以没有 `S`。`R: |-` 里的 `|-` 只是 YAML 的多行字符串写法，真正的返回表达式是里面的 `CASE WHEN`。没就绪时返回 0；如果同一个页面实例刚处理完上一例的事件，读取时已是 `ready = true`、`units = 3`，才返回 3。**这是两个独立入口。** 单独归一化消费入口时，不假定生产入口已经执行；状态取该次消费开始时实际已有的值。

## P 和 E 的分界：这次传进来的，还是程序自己取得的？

`units` 是 P，因为它来自入口调用。形式参数在 YAML 里保留名称、位置和类型，即使后续化简不再用到它，也不改入口签名。代码若把局部形参变量重新赋值，入口的那份 P 仍是调用时的值。对象参数不因此被深冻结；程序对引用对象的读写仍按原语言执行。

`remaining` 是 E，因为调用方并没有把它作为本次参数传进来。`source: src/Dispatch.java#Beans.DISPATCH.remaining` 从源码文件里的静态根 `Beans.DISPATCH` 定位这一个实例的字段。只写 `Dispatch.remaining` 会丢掉实例路径，无法区分同一字段声明在不同实例上的状态。这里写的是来源定位，不是一条直接访问私有字段的 Java 语句。这个例子里的公式用 `remaining` 表示进场初值；第一次写完后，新的字段读取在公式里体现为 `remaining - units`。不能拿新值反过来改写初值。有的 E 来自已声明的数据表，有的来自执行中才发生的环境读取，例如读时钟；它们不变成新增入口参数，也不一律提前读。

**一次外部调用可以同时产生动作和信息。** 创单的动作由 S 的那条 `call` 记录；回执是程序得到的新信息，直接由这条调用的 `result: $e_0` 定义，不再在顶层 E 里声明第二份。同一次回执读两遍，仍是同一个符号；同参数再次调用，是另一次发生，不能共用上一次回执。只读时钟和创单也不同：前者只取得环境信息，不因此多出一个外部写入。

## S 是在发生那一刻留下的快照

S 不是一张「执行完回头看，所有对象现在长什么样」的表。程序走到一次写入或调用时，**先按当时的状态求出写入值或这次实参，再记录这条动作**。后面写同一字段、修改有别名的对象，都不能追溯修改已经记录的内容。

比如同一个对象的 `units` 先是 3，调用 `send(draft.units)` 后改成 5，再调一次 `send(draft.units)`：

| 执行到哪里 | 字段当时的值 | 已记录的对外动作 |
| --- | --- | --- |
| 第一次调用 | 3 | 第一次发送 3 |
| 修改字段 | 5 | 第一次仍是 3 |
| 第二次调用 | 5 | 第一次发送 3，第二次发送 5 |

如果外部合同在调用时把整个请求对象序列化，快照的是当次实际提交的字段值，而不是挂一条可被后续赋值改动的引用。这并不意味着 Java 的每一次对象传参都会深拷贝。若一个异步端口是在稍晚才真正读取共享对象，必须按那个真实读取边界表达，不能把快照前移到调用语句上。

**顺序也属于快照的一部分。** 两次可观察写入即使最后落到同一个值，也不能只记录最后一次；写完再调外部端口，与先调端口再写，不是同一份 PERS。若一个动作只在某些路径上发生，就在其条目记录条件；没执行的路径没有这条动作，不生成一条「写回原值」凑数。只影响本次调用的、不逃逸的局部操作则不进入 S，但仍由内核执行。

## 不只比较返回值

两段程序都返回同一个订单号，还不够：一段可能先扣减再创单，另一段可能创单失败就不扣减。反过来，两个程序源码和局部变量完全不同，只要参数来源、环境依赖、正常或异常完成，以及可观察动作及其发生条件和顺序一致，才有资格说它们表达的是同一套业务行为。

同样，[第零项与首项](05-关系代数.md#第零项何时就是首项)可以在正常完成域共用同一个取值结构，但这不合并完整 PERS。两个入口对同一空集合分别抛越界异常、返回缺省值时，保留各自的完成结果；如果某个入口为了求值先做了整批扫描，也不因 R 共用结构而删除扫描造成的观察或失败。

PERS 把这些差异写到了结果里。看归一化结果时，不只问「最后得到了什么」，还要问：**这个值从哪里来，在它出来以前发生了什么？**
