# 外部操作、状态和完成

源码执行到真实外部调用时，执行器在当前守卫下追加 S 事件。每次调用有独立的完成符号；它可以决定正常返回值、异常类型以及后续操作是否执行。一次有异常的写入及其返回必须引用同一事件。

宿主字段的写入在发生时更新在线存储与可观察序列。局部对象字段用于求值，只有业务可见的值才在出口出现。异步调用和等待分别绑定到实际的操作与完成值，等待前后对可变宿主状态的读取要区分观察时点。

可运行例子见 [SQL 写入](../../design/examples/topics/relational-data/java-static-external-sql.md)、[有界重试](../../design/examples/topics/loops-and-termination/java-loop-bounded-retry.md)和 [Vue 事件](../../design/examples/topics/language-entries/frontend-vo-production-vue.md)。入口、顺序与异常的验收规则见[实时归一化](realtime-normalization.md)。缺少回调发生次数、调度合同或可观测修改范围时，应报告缺口。
