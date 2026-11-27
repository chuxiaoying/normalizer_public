# 使用 Normalizer

**第一次用？先跑通一个例子，再分析自己的项目。** Normalizer 从选定的程序入口提取业务逻辑，输出 PERS：描述入参、环境、返回值和副作用的 YAML。不需要启动原应用。

1. **先试用。** 按[新手入门](getting-started.md)运行 Java、TypeScript 或 Vue 的最小例子。Vue 和 TypeScript 示例已经准备好，不需要先做 AI 预处理。
2. **换成自己的项目。** 按[输入清单](kernel-input.md)确定需要交付的文件。已闭合的 Java 可以原样交付；需要理解框架、补齐类型或重组入口时，把源码、真实入口和观察范围连同[AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md)交给 agent。CLI 不替你调用 AI。
3. **运行并看结果。** 用 [CLI 参考](cli.md)选命令。退出码为 0 时读标准输出中的 PERS；非零时读标准错误中的诊断。没有 PERS 不等于业务返回了空值。

想核对业务含义，从[第七章](../design/chapters/第七章%20·%20PERS%20登场.md)读 PERS；想运行整个仓库的正式回归，看[测试区](../tests/README.md)。
