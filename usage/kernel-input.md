# 准备 CLI 输入

给 Normalizer 的是一个**分析工程根目录**，不是生产工作区的任意上级目录。只放分析入口所需的闭合源码和场景材料，保留相对路径。

| 原始材料 | 准备好的输入 | 下一步 |
|---|---|---|
| 已闭合的 Java | 依赖可见的 Java 源码；入口是可定位的静态方法或静态可达的接收者调用 | `source` |
| JavaScript / TypeScript | 用有效类型检查闭合的严格 `.ts` 模块和导出入口；不接受 `.js`、`.mjs` 或 `.d.ts` | `module` |
| Vue | 原始 `.vue` 供来源定位，加上准备好的严格 `.ts` 分析模块 | `scenario`，选择 VO 生产或消费 |

入口统一写作 `工程相对路径#入口调用`，不附带某次调用的实际参数。准备时先核对可见源码、边界、状态和类型；需要修改时按[AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md)操作。分析副本缺少关键证据时，说明缺口后停止，不用占位值换取运行成功。

命令和约束见 [CLI](cli.md)。`normalizer.kernel-input/1` 是适配器内部交接协议，不是第四种公开输入，CLI 使用者不需要手写；维护者查[实现区](../implementation/contracts/normalized-source-contract.md)。
