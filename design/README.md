# Normalizer 设计

在 macOS 的 Finder 双击 `design/open.command`，或从仓库根目录运行：

```sh
./design/open.command
```

脚本会自动打开本地阅览器。默认端口是 4176；如果被其他程序占用，会换一个可用端口，实际地址显示在终端。再次启动会关闭同一份仓库中已确认的旧阅览器。首次启动需要 Node.js 24 或更高版本和 npm，缺少的项目依赖会自动安装，不需要构建 Rust 内核。保留启动窗口即可继续浏览，按 Ctrl+C 停止。更多说明见[阅览器文档](../implementation/apps/design-reader/README.md)。

## 一级设计

八章是业务语义和归一化规则的裁决依据。

- [第一章 · JVM 模拟器：执行即归约](chapters/第一章%20·%20JVM%20模拟器：执行即归约.md)
- [第二章 · 多重宇宙的分裂坍缩](chapters/第二章%20·%20多重宇宙的分裂坍缩.md)
- [第三章 · 数学化简：递归地应用定律](chapters/第三章%20·%20数学化简：递归地应用定律.md)
- [第四章 · AI 救赎](chapters/第四章%20·%20AI%20救赎.md)
- [第五章 · 关系代数：让 SQL 和集合运算一起化简](chapters/第五章%20·%20关系代数：让%20SQL%20和集合运算一起化简.md)
- [第六章 · 业务至上](chapters/第六章%20·%20业务至上.md)
- [第七章 · PERS 登场](chapters/第七章%20·%20PERS%20登场.md)
- [第八章 · 统一定形规范](chapters/第八章%20·%20统一定形规范.md)

## 二级设计

八章讲设计主张和分工；这里按主题查边界、写法和代表规则。

- [分析副本怎样通过校验](secondary/分析副本怎样通过校验.md)
- [标准操作与声明标记](secondary/标准操作与声明标记.md)
- [关系指令的统一写法](secondary/关系指令的统一写法.md)
- [回执与异常怎样进入 PERS](secondary/回执与异常怎样进入%20PERS.md)
- [完整二分 CASE 怎么选方向](secondary/完整二分%20CASE%20怎么选方向.md)
- [观察身份与稳定编号](secondary/观察身份与稳定编号.md)
- [什么时候可以引入 LET](secondary/什么时候可以引入%20LET.md)
- [精确数值与区间怎样化简](secondary/精确数值与区间怎样化简.md)
- [有序序列的长度与首项](secondary/有序序列的长度与首项.md)
- [CASE 的取得域与等价化简](secondary/CASE%20的取得域与等价化简.md)


## 设计种子

[种子目录](examples/README.md)用于查看规则的具体例证。种子及其独立预期用于派生验收，不另行定义规则。代码和工程职责请查[实现区](../implementation/README.md)，运行验收请查[测试区](../tests/README.md)。
