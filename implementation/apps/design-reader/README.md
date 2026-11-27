# 设计区阅览器

启动入口、端口和依赖的使用说明见[设计首页](../../../design/README.md)。这里说明阅览器的界面、离线导出与维护验收。

左侧目录按“基础主题／联合主题 → 具体主题 → 种子案例”折叠展示。展开主题后可打开主题导读和案例；搜索会自动展开匹配的主题。

在种子页点击“关联测试”，按入口选择原例或变体，即可查看每个用例的完整预处理后输入及预期 PERS。预期拒绝的用例显示 CLI 应报告的诊断。可以收藏单条用例，也可以在目录中按变体名搜索。种子清单列出每个种子的测试数或“待派生”状态。

需要离线阅读时，在仓库根目录运行下列命令。首次导出需安装阅览器依赖，以及读取关联用例所用的 YAML 依赖；已通过启动脚本安装过的检出可跳过前两行。

```sh
npm --prefix implementation/apps/design-reader ci --ignore-scripts --no-audit --no-fund
npm --prefix implementation/tools/frontend ci --ignore-scripts --no-audit --no-fund
reader_out=$(mktemp -d)
node implementation/apps/design-reader/reader.mjs --export "$reader_out/normalizer-design.html"
```

离线文件包含导出时的种子、关联测试详情和参考页，用浏览器打开该文件即可离线阅读。导出路径的父目录须存在；HTML 是本地生成物，不提交到 Git。
