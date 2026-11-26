---
name: normalizer-preprocess
description: 根据 Normalizer 的设计种子，由 AI 阅读原业务和依赖证据，交付保持原语言的闭合输入；不依赖专用预处理工具，不预先计算 PERS。
---

# Normalizer AI 预处理

AI 负责理解和改写，不依赖仓库的盘点器、框架插件或 JAR 工具。先选 [设计种子](../../../design/examples/README.md)，对照其原始源码、AI 预处理差异及独立预期。一个做法没有种子证明，就不要当作已支持的交付方式；先提出最小种子，由独立预期说明原行为和应有结果。输入是否准备好不等同内核已经支持，见[覆盖索引](references/seed-coverage.md)。

1. **圈定真实入口和观察范围。** 阅读入口可达的本地代码、类型及调用关系。能原样交付就不改，见[加一](../../../design/examples/topics/expression-simplification/java-static.md)和[多层调用](../../../design/examples/topics/expression-simplification/java-static-deep-calls.md)。参数和返回类型从声明取得，不从名字猜测；`today` 等入口入参在调用内不变。
2. **只改需要闭合的地方。** 保持原语言语法，保留数据流、状态、异常、别名、次数和顺序。用原始语句保留业务计算；不要预填常量答案、新增结果入参或用空集合替代未知。需要等价表示时提供能计算的定义，如[文本库](../../../design/examples/topics/external-boundaries/java-external-library.md)、[有限循环](../../../design/examples/topics/loops-and-termination/java-variable-finite-bound.md)及[SQL 查询和写入](../../../design/examples/topics/relational-data/java-static-external-sql.md)。完整标记见[规范源码合同](../../contracts/normalized-source-contract.md)。
3. **如实交付边界。** 有源码就保留实际实现；只有真实且已声明的 external call 才进入 S。普通对象和接口签名不提供实现，不准标成端口绕过拒绝。不可闭合时明确列出缺失合同和源码位置；若非 external 的无源码调用到达内核，内核 Panic，不产出 PERS。见[对象分派](../../../design/examples/topics/objects-and-types/java-polymorphic-dispatch.md)和[外部调用异常](../../../design/examples/topics/external-boundaries/java-exception-propagation.md)。
4. **提交最小、可审阅的交付物。** 交付规范源码或原源码加局部 diff、真实入口、类型和边界、观察范围，以及改写与原始行为等价的依据。Vue 须保留原组件和预处理后的 `.mjs`：消费侧标明 VO 读取，生产侧保留等待、宿主读取时点和有序写入。前者见[消费](../../../design/examples/topics/language-entries/frontend-vo-consumption-vue.md)，后者见[生产](../../../design/examples/topics/language-entries/frontend-vo-production-vue.md)。
5. **验收与预处理分开。** 从原源码独立推导完整 PERS 或精确拒绝预期，再用[正式 CLI](../../../usage/cli.md)比较。正式测试回放给定的 AI 交付物，不证明 AI 每次预处理都正确；补丁能应用也不表示该种子的 CLI 行为已通过。见[测试说明](../../../tests/README.md)。
