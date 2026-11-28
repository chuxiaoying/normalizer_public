# CLI 参考

在 Normalizer 仓库根目录运行以下命令。先按[新手入门](getting-started.md)装好工具，再按[输入清单](kernel-input.md)准备分析工程。CLI 只读取交付物，不调用 AI。

| 准备好的输入 | 命令 |
|---|---|
| Java 源码工程 | `implementation/tools/normalizer source PROJECT_ROOT 'src/Entry.java#Entry.run'` |
| 独立 TypeScript 模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/module.ts#exportedFunction'` |
| 前端事件或属性观察的严格 TypeScript 模块 | `implementation/tools/normalizer scenario SCENE PROJECT_ROOT --entry 'src/page.ts#exportedFunction'` |
| 本仓正式测试 | `implementation/tools/normalizer test business-examples`，见[测试区](../tests/README.md)。 |

`PROJECT_ROOT` 指**分析工程根目录**。例如其中的文件是 `src/Entry.java`，第一个参数传工程目录，入口仍写 `src/Entry.java#Entry.run`，不要把根目录传成 `src/`，也不要在入口写绝对路径、`./` 或实际调用值。用引号包住定位符。Java 重载确有歧义才写源码参数类型，例如 `'src/Add.java#Add.run(int, int)'`；静态接收者可写 `'src/Beans.java#Beans.SERVICE.run'`。入口不是每次运行都提供一组入参的命令；CLI 分析该入口的符号输入。

### 源文件与类型检查

- `source` 读取分析工程目录下的 Java 源文件。只提交选定入口所需的工程文件，使用 UTF-8。目录软链接会被拒绝，文件软链接只能指向工程内部。
- `module` 和前端 `scenario` 接收可执行的严格 `.ts` 文件。适配器拒绝 `.js`、`.mjs`、`.d.ts`、`any` 和压制检查的注释。

### 当前实现限制

- **TypeScript 导入。** 独立模块按固定严格选项检查源码，可用受支持的项目相对导入，包括 `import name from "./helper.js"` 和 `import { default as name } from "./helper.js"`。原工程的 `tsconfig.json` 和宿主模块初始化不会执行。循环依赖、副作用导入、越界路径不受支持。跨文件帮助函数目前限可静态定位的同步纯标量计算。
- **TypeScript 入口。** 独立模块可选有函数体的导出函数（默认导出写 `#default`），或同文件导出的只读字面量对象方法，如 `src/main.ts#controller.read`。动态装配、脱离对象的方法和导出的箭头函数变量不受支持。
- **已接入的操作。** 独立模块可从官方 `@normalizer/stdlib` 命名空间使用 `trim` 和 `defaultIfBlank`。时钟操作当前可用于 Java，TypeScript 适配器尚未接入。

## 前端 VO 场景

`--entry` 指向分析工程内的 TypeScript 导出函数。原始 `.vue`、`.axml` 和组件配置留在原工程。

| 第九章选定的场景 | CLI 场景名 |
|---|---|
| [前端生产](../design/chapters/09-场景归一.md#前端生产允许改什么) | `frontend-vo-production` |
| [前端消费](../design/chapters/09-场景归一.md#前端消费允许改什么) | `frontend-vo-consumption` |

每次命令只输出所选入口的结果。[分析工程目录与入口写法](kernel-input.md)

- **生产适配器**接收具名导出入口：同步 `void` 入口可以读写已声明的 VO；`async` 入口可以直接等待已声明的外部端口。
- **消费适配器**可处理标量常量、已有状态字段和受支持的条件选择。复合挂载条件可能被拒绝，例如 `ready && (status === "open" || status === "pending")`。

直接可运行的两条完整命令见[新手入门](getting-started.md#3-前端-vo运行生产入口与消费入口)。

## 诊断

先看退出码。三个分析命令成功时从标准输出（stdout）读取 PERS YAML；遇到失败，查看标准错误（stderr）。`test business-examples` 输出测试报告。重定向可能预先创建空文件，文件存在不代表命令成功。

| 遇到的失败 | 下一步 |
|---|---|
| 构建失败、依赖缺失或命令用法错误 | 先查 Cargo、Node/npm、网络或缓存，以及命令参数顺序；尚未得到业务结果。 |
| 入口或源码校验未通过 | 只核对分析工程的根目录、源码扩展名和大小写、入口定位和所需声明。不要为排错把原始框架文件加入工程。 |
| 分析未完成或预算不足 | 记录命令、入口和诊断，标明这次未取得完整结果。 |
| 诊断要求预处理或不支持 | 保留具体位置及 stderr，按[预处理规则](../design/chapters/04-AI救赎.md#ai-05-预处理停在哪儿)区分输入证据缺失和当前实现不支持。前者补相应资料，后者保留复现。前端错误即使提到内部工程或 Java，也应提交原 TypeScript 输入和诊断。 |

命令成功但 YAML 里出现 `throws` 时，按[第七章](../design/chapters/07-PERS登场.md#pers-03-完成与完整等价)解读结果。
