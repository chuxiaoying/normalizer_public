# CLI 参考

从仓库根目录执行。首次使用时需要相应工具链与依赖。

| 输入 | 命令 |
|---|---|
| 已闭合的 Java 源目录 | `implementation/tools/normalizer source SOURCE_DIR Class.method` |
| 独立 JavaScript 模块（当前成功输出限闭合同步标量） | `implementation/tools/normalizer module PROJECT_ROOT src/module.js exportedFunction` |
| 预处理后的 Vue 模块及原组件 | `implementation/tools/normalizer scenario SCENE PROJECT_ROOT --entry ENTRY_FILE:FUNCTION --source ORIGINAL.vue` |

用 `implementation/tools/normalizer help` 查看入口。成功时从 stdout 读取最终 PERS，失败时查看退出码和 stderr 中的诊断。见[Java 入门](getting-started.md)。

独立模块的普通对象入参没有 external 合同。对入口立即执行、无可选链的入参方法调用，模块适配仅交付实际调用目标供内核检查，包括直接 `await input.read()` 的情况；没有源定义时 Panic，不产出 PERS。不把 JSDoc 或 `ports` 等入参名当作边界声明，也不据此宣称支持通用异步执行。
