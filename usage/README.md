# 使用 Normalizer

**先准备分析副本，再运行 CLI。** Normalizer 不会替你调用 AI，也不会运行 Spring、Vue 或原业务应用来补出隐藏逻辑。已经闭合的 Java 不必改写；原始 JavaScript 要准备成严格 TypeScript，Vue 要保留原组件和准备好的 TypeScript 模块。

1. **准备输入。** 阅读[输入清单](kernel-input.md)。需要理解工程、补齐类型或重组入口时，把[AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md)交给处理该工程的 agent，连同源码、真实入口和观察范围。
2. **装工具并跑通。** 从仓库根目录按[新手入门](getting-started.md)跑 Java、TypeScript 或 Vue 的最小可复现例子。
3. **对照自己的项目。** 用 [CLI 参考](cli.md)选命令，成功时读取 stdout 的 PERS，失败时根据退出码和 stderr 定位。不把缺少 PERS 理解为业务空值。

想核对业务含义，从[第七章](../design/chapters/第七章%20·%20PERS%20登场.md)读 PERS；想运行整个仓库的正式回归，看[测试区](../tests/README.md)。
