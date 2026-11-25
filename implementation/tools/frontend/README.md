# JavaScript、TypeScript 与 Vue 的语言适配

此处是正式 CLI 的语言适配，不是 AI 预处理工具。
- `standalone.mjs` 把已闭合的独立 JS 模块交给共享内核。[模块种子](../../../design/examples/topics/language-entries/javascript-module.md)列出已认证的标量、只读 Vector 和有序文本 Map；适配层只校验源码形态，CASE 的规范化在内核完成。
- `typed-module.mjs` 共享 TypeScript 原始语法和类型检查；`module-tuple.mjs` 交付局部元组观察，`module-listener.mjs` 交付可见、未逃逸回调的实际派发。内核求值并记录真实外部交互；语言适配不会执行输入模块。
- `module-event.mjs` 与 `module-projection.mjs` 解析事先交付的 Vue 规范模块，由场景适配器消费。
- `ir.mjs` 与 `local-directives.mjs` 提供这些入口共用的表达式和标记处理。

用户使用[统一 CLI](../../../usage/cli.md)，AI 自行阅读并闭合业务，参照[设计种子](../../skills/normalizer-preprocess/references/seed-coverage.md)交付。
