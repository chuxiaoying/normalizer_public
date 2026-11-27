# TypeScript 与 Vue 的 CLI 适配

此处是正式 CLI 的语言适配，不是 AI 预处理工具。

- `standalone.mjs` 是 `module` 入口，检查严格 TypeScript，并读取同一文件中有函数体的具名导出声明。可运行的入口形态、单模块检查及导入限制见[CLI 参考](../../../usage/cli.md#当前实现限制)；共享内核负责 CASE 化简与定形。
- `typed-module.mjs` 共享 TypeScript 原始语法和类型检查；`module-tuple.mjs` 交付局部元组观察，`module-listener.mjs` 交付可见、未逃逸回调的实际派发。内核求值并记录真实外部交互；语言适配不会执行输入模块。
- `module-event.mjs` 与 `module-projection.mjs` 解析事先交付的 Vue 规范模块，由场景适配器消费。
- `ir.mjs` 与 `local-directives.mjs` 提供这些入口共用的表达式和标记处理。

用户使用[统一 CLI](../../../usage/cli.md)，原始 JavaScript 的处理走[预处理 Skill](../../../usage/skills/normalizer-preprocess/SKILL.md)，不由此目录的脚本完成。
