# 预处理做法与设计种子

本表把 AI 应交付的改写连到可审阅的种子，不是另一份测试清单。状态以 [CLI 测试目录](../../../../tests/e2e/catalog.yaml)为准；`pending` 种子说明设计意图，不代表正式 CLI 已通过。

| 预处理交付 | 有正式 CLI 用例的种子 | 只有设计证据的典型边界 |
|---|---|---|
| 原样交付纯计算、可达 helper 和完整类型 | [加一](../../../../design/examples/series-1/java-static.md)、[多层调用](../../../../design/examples/series-1/java-static-deep-calls.md)、[对象构造](../../../../design/examples/series-1/java-static-object.md) | — |
| 交付等价计算，不预填答案 | [文本库替换](../../../../design/examples/series-1/java-external-library.md)、[精确小数及舍入模式](../../../../design/examples/topics/number-domain/java-decimal-rounding.md) | — |
| 保留状态、可空类型、别名和对象投影 | [状态与参数同名](../../../../design/examples/series-1/java-name-collision.md)、[可空字段](../../../../design/examples/topics/objects-and-types/java-optional-field.md)、[集合别名](../../../../design/examples/topics/collections/java-alias-mutation.md)、[有环投影拒绝](../../../../design/examples/topics/state-and-aliasing/java-cycle-observable.md)、[对象状态别名](../../../../design/examples/series-2/java-state-alias.md)、[键存在但值为空](../../../../design/examples/series-2/java-map-present-null.md) | — |
| 声明真实 external、正常和异常完成、边界顺序 | [外部对象返回](../../../../design/examples/topics/external-boundaries/java-external-object.md)、[异常](../../../../design/examples/topics/external-boundaries/java-exception-propagation.md)、[有序同步调用](../../../../design/examples/topics/external-boundaries/java-external-callback-order.md)、[无源码接口对照](../../../../design/examples/topics/objects-and-types/java-polymorphic-dispatch.md) | [TS 回调登记与派发](../../../../design/examples/series-2/frontend-deferred-listener.md) |
| 交付关系读写、边界、选取顺序及回执 | [SQL 三个入口](../../../../design/examples/series-1/java-static-external-sql.md)、[有界重试](../../../../design/examples/series-1/java-loop-bounded-retry.md)、[continue/break](../../../../design/examples/topics/loops-and-termination/java-break-continue.md)、[无界轮询拒绝](../../../../design/examples/series-1/java-loop-unknown.md)、[预先获取首项](../../../../design/examples/series-2/java-eager-first.md)、[投影去重](../../../../design/examples/series-2/sql-projected-distinct.md) | — |
| 保留真实入口、异步等待、VO 读写和观察时点 | [Vue 生产](../../../../design/examples/series-1/frontend-vo-production-vue.md)、[Vue 消费](../../../../design/examples/series-1/frontend-vo-consumption-vue.md)、[独立模块](../../../../design/examples/topics/language-entries/javascript-module.md)、[独立 JS 无源码拒绝](../../../../design/examples/topics/language-entries/javascript-async-function.md)、[TS tuple 槽位](../../../../design/examples/series-2/typescript-tuple-slots.md) | — |

没有对应种子的能力不列入当前交付流程。例如 Maven/JAR 专门取证、整项目 TSX/小程序模板识别、通用 JSON 转换不能从表里的库函数或 Vue 案例类推。实际需要时，先设计最小的原始调用、局部交付、可观察判别例与独立预期，再审查正式支持范围。
