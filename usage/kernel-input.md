# 准备 CLI 输入

为选定的入口准备一个分析工程目录，按项目相对路径放入入口、它用到的源码及声明。原框架工程留作查证，预期 PERS 和运行输出放在输入目录外。如果需要 AI 预处理，按第九章对应场景准备，再核对工具目前接收的输入。[场景预处理](../design/chapters/09-场景归一.md#规则区) · [分析副本校验](../design/chapters/04-AI救赎.md#ai-11-分析副本先通过静态校验) · [CLI 当前限制](cli.md#当前实现限制)

把 AI 预处理后的零包依赖工程目录交给 CLI。`package.json`、`pom.xml` 可以省略；已有清单只留下无依赖的内容。依赖尚未处理完就回到[第四章 AI-11](../design/chapters/04-AI救赎.md#ai-11-分析副本先通过静态校验)完成预处理。

| 已准备的文件 | 运行方式 |
|---|---|
| Java 工程，如 `src/Entry.java` 及相关类 | `source`，指定 `'src/Entry.java#Entry.run'` |
| 严格 TypeScript 模块，如 `src/module.ts` 及其相对导入 | `module`，指定导出函数或受支持的接收者方法 |
| 从页面提取的严格 TypeScript 模块，如 `src/page.ts` | `scenario`，指定生产或消费场景及导出函数 |

传给命令的目录是上表中 `src/` 的上一级。可以在[新手入门](getting-started.md)里复制一个完整输入，再按[CLI 参考](cli.md)换成自己的入口。

一次调用指定一个入口。要保留结果，可将 stdout 和 stderr 分别重定向到分析目录以外的文件。查看[入门示例](getting-started.md#4-辨认失败)中的完整命令。
