# 从业务代码到 PERS

我们希望从一段业务代码读出一条规则：给定哪些输入，会返回什么，改动什么状态，调用谁，又会在什么条件下失败？设计上，这个过程分三步。

```text
原始项目 → AI 局部预处理 → 保持原语言语法的规范源码
         → 各语言解析、绑定 → 统一 IR
         → 内核边执行边归一化 → 最终 PERS
```

## AI 改哪几行就够了？

Java 入口用 Apache 的 `StringUtils.trim(raw)` 裁剪文本。若工程为每个库各写一套规则，换个库就要再教内核一次。这里让 AI 把调用目标换成 Normalizer 已定义的文本操作，方法的嵌套和返回位置都沿用原代码。输入 `"  A  "` 应得到 `"A"`；输入只有空格且备用文本为 `"B"` 时应得到 `"B"`。业务期望能直接检查这两次调用的组合。[完整案例](examples/topics/external-boundaries/java-external-library.md)

多语言也沿用这条分工。Java 可以用注解标明某次调用的外部端口，TypeScript 可以在端口声明上加标记注释。各自的解析器理解本语言的语法，把它们交付成统一 IR 中同一种外部调用。内核只需知道哪个端口、哪些参数、在什么条件下被调用。例如 TypeScript 的 `handle(false)` 只登记回调，`handle(true)` 才会在登记之后发送通知。登记和实际调用的区别必须一路保留到 PERS。[Java 端口与重试](examples/topics/loops-and-termination/java-loop-bounded-retry.md) · [TypeScript 登记与派发](examples/topics/external-boundaries/frontend-deferred-listener.md)

## 符号化执行为什么需要多个“宇宙”？

看 `Decision.choose(value, limit, fallback)`。三个参数都是这次执行给定的输入。它们的具体数值可以未知；每次引用同一个参数，都取同一个符号值。内核按源码执行到 `adjusted = value + 1`，后续读到 `adjusted` 时就能使用这个表达式。遇到 `adjusted > limit`，已知条件还不足以判定真假，于是**分裂**：在条件为真和为假的两个宇宙里，分别沿可达代码继续走。为真的一边已经返回；只有为假的一边还需检查是否相等。[完整案例](examples/topics/control-flow/java-static-case-when.md)

| 此时已知的条件 | 走到返回时，内核已算出的值 |
|---|---|
| `limit < value + 1` | `value + 1` |
| `limit == value + 1` | `fallback` |
| `value + 1 < limit` | `(value + 1) - 1` 化成 `value` |

在能汇合的位置，内核按每条路径的条件收拢仍可继续的状态。已经返回或抛出的路径在对应出口收拢。这就是**坍缩**：相同的值可以合一，不同的业务结果要保留选择条件。上表三个正常返回最终写成一条 `R: CASE … END`。**实时在线归一化**要求每一步算出的值、条件和状态随执行更新，下一步立即使用；最终的 PERS 展示这些值在业务出口的含义。

异常路径尤其能检验这个要求。若 `quota.units = 7` 之后执行 `return observer.units`，传入同一个对象会读到 7，传入另一个初值为 3 的对象会读到 3，传入空值则抛出异常。三条路径里额度都已写成 7。坍缩时，既要把异常与正常返回分开，也要保住异常之前已经发生的写入。[先写再读的额度案例](examples/topics/state-and-aliasing/java-state-alias.md)

## 数学化简能揭示什么业务规则？

有个方法用了加减法、两个不等式、一个等式和一个布尔开关。对照源码，是否返回基础值，其实只取决于一对数。[完整案例](examples/topics/expression-simplification/java-expression-simplification.md)

| 源码中的关系 | 内核应当推出什么 |
|---|---|
| `x + y <= 10` 且 `x + y >= 10`，同时 `x - y == 2` | 和为 10，差为 2，唯一解是 `x == 6` 且 `y == 4`。 |
| `(eligible && preferred) \|\| (eligible && !preferred)` | 无论开关取真还是取假，整个条件都等于 `eligible`。 |
| 命中时返回 `(base + x) - x` | 在业务数值域中，加上的量抵消，返回 `base`。 |

因此 `base = 9, x = 6, y = 4` 时，无论开关如何都返回 9；把 `y` 改成 5 就返回 0。最终表达只需写一次条件。对允许交换的运算，等价写法也按同一规则确定次序，让两个语言写出的相同业务能够逐项对齐。开关依然属于入口参数，它对这个结果没有影响，正是化简揭示出的业务事实。
