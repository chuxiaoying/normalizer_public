# 场景入口层、统一 IR 与归一化内核

[AI救赎](../../design/chapters/AI救赎.md)定义 AI 预处理的职责，[符号执行](../../design/chapters/符号执行.md)和[PERS登场](../../design/chapters/PERS登场.md)分别定义执行和结果。这里维护工程接线和责任层，不另定输入或输出语义。

| 环节 | 负责什么 | 工程入口 |
|---|---|---|
| AI 预处理 | 人或 agent 在 CLI 之外交付闭合的分析副本和来源证据 | [预处理 Skill（使用区）](../../usage/skills/normalizer-preprocess/SKILL.md) |
| 场景/语言解析 | Java 读规范源码、绑定入口；TypeScript 和 Vue 在适配层消化语法、类型和场景差异，再交给共享内核输入。 | `tools/normalizer`、`tools/frontend/`、`tools/scenarios/`、`crates/simulator-parser/` |
| 归一化内核 | 校验闭合交接，执行统一 IR、化简，生成最终 PERS | `crates/normalizer-ir/`、`crates/math/`、`crates/pipeline/` |

`tools/` 相对 `implementation/`，`crates/` 相对 `implementation/packages/normalizer/`。具体生产路径见[源码交接](../contracts/normalized-source-contract.md)与[输出机器接口](../contracts/business-pers.md)。公开使用方只调用[CLI](../../usage/cli.md)的 `source`、`module` 或 `scenario`；它们读取已经准备好的分析副本，不启动 AI、不执行原始宿主应用。场景区分留在适配层，公用规则不按场景名或语言名分叉。

改动接线时分别验证解析和加载、真正执行及最终发布。结构检查通过只能证明交接结构；完整 PERS 或拒绝必须用[正式 CLI 回归](../../tests/README.md)验收。诊断归属和排查顺序见[维护路由](agent-maintenance.md)。
