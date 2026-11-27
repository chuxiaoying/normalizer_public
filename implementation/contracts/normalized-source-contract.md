# 规范源码如何交给解析器与内核

本页供维护解析器、场景适配和内核加载的人使用。输入应具备什么语义，以[第四章《AI 救赎》](../../design/chapters/第四章%20·%20AI%20救赎.md)和[分析副本校验规则](../../design/secondary/分析副本怎样通过校验.md)为准；标记和关系指令分别查[标准操作与声明标记](../../design/secondary/标准操作与声明标记.md)、[关系指令的统一写法](../../design/secondary/关系指令的统一写法.md)。这些规则不在这里重新列一份。

| 交接 | 看哪里 | 保留的工程信息 |
|---|---|---|
| Java 源码 → 入口和闭合调用 | `crates/simulator-parser/src/`、`crates/pipeline/src/normalized.rs`、`crates/pipeline/src/kernel_input/source_entry.rs` | 解析时保留所属类型、源码位置和参数签名。解析失败沿用原文件及可用行列；部分静态绑定诊断只给出入口或方法身份和失败原因，不保证每条都有源码行列。 |
| 独立模块 / Vue → 内部工程数据 | [TypeScript 适配](../tools/frontend/README.md)、`tools/scenarios/module-project.mjs` | 从已准备的严格 `.ts` 封装有类型的 `normalizer.kernel-input/1` JSON。Vue 原件用于来源定位：检查路径、存在性和可读性，不解析原件，也不验证改写等价。 |
| 内部工程数据 → 内核输入 | `crates/pipeline/src/kernel_input.rs`、`crates/normalizer-ir/src/local.rs` | 核对 schema、声明绑定和表达式类型；这些节点与诊断属于内部交接，不是最终 PERS。 |

`crates/` 相对 `implementation/packages/normalizer/`，`tools/` 相对 `implementation/`。用户命令统一查[CLI 参考](../../usage/cli.md)；内部 JSON 不是额外的公开 CLI。修改任一交接字段，要同时核对生成端、加载端、失败阶段和诊断所能提供的源码定位，再以种子派生的完整 PERS 验收。语言特定的源码证据保留在对应入口，不为同一语义增设第二套 PERS 格式。

## 内部适配信封

以 `kernel_input.rs` 的 `Project`、入口上下文类型及正式回归为准。适配方同时提供原业务入口与独立签名载体：`source`、`source_text` 和 `functions` 供内部装载，`entry_contexts` 把入口和声明绑定回业务来源。`parameter_value_types` 与 `result_value_types` 分别覆盖形参与结果的载体检查，不能互为别名。局部关系指令在装载时成为结构化查询，执行再按真实行绑定检查类型。

排查时核对入口上下文是否齐全、参数位置是否连续、外部类型的字段是否闭合。不能从其他入口借同名声明；参数与结果的绑定以显式字段为准，不依赖 JSON 键顺序。样例载体见场景适配器，工程验收见[验证入口](../docs/validation.md)。
