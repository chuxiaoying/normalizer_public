# CLI 参考

从仓库根目录执行。首次使用时需要相应工具链与依赖。

| 输入 | 命令 |
|---|---|
| 已闭合的 Java 源目录 | `implementation/tools/normalizer source SOURCE_DIR 'src/Entry.java#Entry.run'` |
| 严格 TypeScript 标量或只读值模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/module.ts#exportedFunction'` |
| 独立 TypeScript 局部元组模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/receipt.ts#traceCount'` |
| 独立 TypeScript 同步监听模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/handler.ts#handle'` |
| 预处理后的 Vue 模块及原组件 | `implementation/tools/normalizer scenario SCENE PROJECT_ROOT --entry SOURCE.ts#FUNCTION --source ORIGINAL.vue` |

用 `implementation/tools/normalizer help` 查看入口。成功时从 stdout 读取最终 PERS，失败时查看退出码和 stderr 中的诊断。见[Java 入门](getting-started.md)。

输入文件必须留在所选的源码目录内。Vue 的原组件与预处理模块分别校验；越界会报错，不产出 PERS。指向目录内文件的链接保留调用时声明的相对路径；Java 源目录扫描不会递归进入目录链接。

`module` 只接收可执行的 `.ts` 源文件，不接收 `.js`、`.mjs` 或类型声明文件 `.d.ts`。原始 JavaScript 须先由 AI 按真实合同准备成 TypeScript；只改文件后缀或补 JSDoc 不算完成。CLI 用固定的 TypeScript 严格选项检查整个准备模块（包括未调用的帮助函数），拒绝显式 `any`、隐式 `any` 和从表达式或类型中解析出的 `any`，也不允许用注释关闭或压制类型检查。失败时不调用内核、不产出 PERS。Vue 的准备 `.ts` 模块也适用同一规则。通过类型检查不代表所有 TypeScript 写法都属于已实现的归一化合同。

独立模块的普通对象入参没有 external 合同。对入口立即执行、无可选链的入参方法调用，模块适配仅交付实际调用目标供内核检查，包括直接 `await input.read()` 的情况；没有源定义时 Panic，不产出 PERS。不把 JSDoc 或 `ports` 等入参名当作边界声明，也不据此宣称支持通用异步执行。
