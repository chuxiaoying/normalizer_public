# 设计区阅览器

在 macOS 上双击 [open.command](open.command)，或从仓库根目录运行：

```sh
implementation/apps/design-reader/open.command
```

浏览器打开 [本地阅览器](http://127.0.0.1:4176/)。首次运行需要 Node.js 24 或更高版本和 npm，启动脚本会安装锁定的依赖。服务只监听本机。保留启动窗口，刷新即可读取最新的设计与测试。

在种子页点击“关联测试”，按入口选择原例或变体，即可查看具体文件的源码差异和完整预期 PERS。预期拒绝的用例显示 CLI 应报告的诊断。可以收藏单条用例，也可以在目录中按变体名搜索。种子清单列出每个种子的测试数或“待派生”状态。

需要离线阅读时，可在安装过依赖后导出一个独立的 HTML 文件：

```sh
node implementation/apps/design-reader/reader.mjs --export "$HOME/Downloads/normalizer-design.html"
```

离线文件包含导出时的种子、关联测试详情和参考页，重新导出即可更新。
