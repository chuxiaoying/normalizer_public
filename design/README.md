# Normalizer 设计

在 macOS 的 Finder 双击 `design/open.command`，或从仓库根目录运行：

```sh
./design/open.command
```

脚本会自动打开本地阅览器。默认端口是 4176；如果被其他程序占用，会换一个可用端口，实际地址显示在终端。再次启动会关闭同一份仓库中已确认的旧阅览器。首次启动需要 Node.js 24 或更高版本和 npm，缺少的项目依赖会自动安装，不需要构建 Rust 内核。保留启动窗口即可继续浏览，按 Ctrl+C 停止。更多说明见[阅览器文档](../implementation/apps/design-reader/README.md)。

## 设计章节

各章及其规则区是业务语义和归一化规则的裁决依据。正文讲主张，章末规则区列代表性的细则，每条配示例说明输入、关键前提和处置结果。规则区不是穷举；未列情形仍按章内主张和已确立的业务合同裁决。

- [符号执行](chapters/01-符号执行.md)
- [多重宇宙](chapters/02-多重宇宙.md)
- [数学化简](chapters/03-数学化简.md)
- [AI救赎](chapters/04-AI救赎.md)
- [关系代数](chapters/05-关系代数.md)
- [业务至上](chapters/06-业务至上.md)
- [PERS登场](chapters/07-PERS登场.md)
- [统一定形](chapters/08-统一定形.md)
- [场景归一](chapters/09-场景归一.md)

## 设计种子

[种子目录](examples/README.md)用于查看规则的具体例证。种子及其独立预期用于派生验收，不另行定义规则。代码和工程职责请查[实现区](../implementation/README.md)，运行验收请查[测试区](../tests/README.md)。
