# Normalizer

Normalizer 从选定的业务入口出发，归一化参数、返回值、状态变化、外部调用和异常，输出 PERS。

| 要做什么 | 从这里开始 |
|---|---|
| 用当前 CLI 跑一段源码，了解输入要求和实际输出 | [使用](usage/README.md) |
| 审阅原始代码、AI 预处理与理想 PERS，裁决共同规则 | [设计](design/README.md) · [本地 HTML 阅览器](implementation/apps/design-reader/README.md) |
| 找到预处理 Skill、语言适配器、内核及构建入口 | [实现](implementation/README.md) |

[独立测试区](tests/README.md)按种子管理派生用例，从正式 CLI 核对完整输出或拒绝原因。参与开发前请阅读[仓库约定](AGENTS.md)。
