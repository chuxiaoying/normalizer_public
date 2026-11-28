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

- Java 和 TypeScript 适用同一套[合法引用白名单](../design/chapters/04-AI救赎.md#合法引用白名单)。主错误码或诊断详情出现 `NORM-IMPORT` 时，按提示位置核对实际导入或类型绑定。没有导入的调用仍须有已定义语义或真实外部合同；是否走到调用点由执行路径决定。

### 编译与 lint 检查

TypeScript 用仓库固定的严格编译配置做静态校验，并检查 Normalizer 的准入规则，例如禁止 `any`、压制检查和加载宿主声明。不读取原工程的 ESLint 或 tsconfig，也不运行源码。看到 `tool_code=TS…` 时，用它和 `location` 定位，但按 Normalizer 主错误码处理。

### 当前实现限制

- **TypeScript 导入。** 独立模块按固定严格选项检查源码，可用受支持的项目相对导入，包括 `import name from "./helper.js"` 和 `import { default as name } from "./helper.js"`。原工程的 `tsconfig.json` 和宿主模块初始化不会执行。循环依赖、副作用导入、越界路径不受支持。跨文件帮助函数目前限可静态定位的同步纯标量计算。
- **TypeScript 入口。** 独立模块可选有函数体的导出函数（默认导出写 `#default`），或同文件导出的只读字面量对象方法，如 `src/main.ts#controller.read`。动态装配、脱离对象的方法和导出的箭头函数变量不受支持。
- **已接入的操作。** 独立模块可从官方 `@normalizer/stdlib` 命名空间使用 `trim` 和 `defaultIfBlank`。时钟可用于 Java；TypeScript 独立模块可用官方 `N.currentTimeMillis()`，通过链接帮助函数传递观察仍受当前适配边界限制。

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

## 失败时怎么处理

**先看退出状态，再读 stdout。** `source`、`module`、`scenario` 成功才解析完整 PERS。失败时看 stderr；不能把现有的空输出文件或日志当成结果。正常 PERS 可以包含 `throws` 或 `R: void`，这是业务完成轨迹，[不是 CLI 错误](../design/chapters/07-PERS登场.md#pers-03-完成与完整等价)。

完整[错误码、解释和处理建议](../implementation/docs/cli-error-codes.md)也可用 `implementation/tools/normalizer errors` 查看。文本错误首行给出 `[错误码] kind=类别 stage=阶段`。例如：

| 看到什么 | 先做什么 |
| --- | --- |
| `NORM-USAGE`、`NORM-TOOLCHAIN` 或临时目录失败 | 先检查命令、Cargo、Node/npm 或权限。不要改业务输入。 |
| 清单、入口、源码或绑定诊断 | 按位置回查输入合同，补真实源码或可靠的外部合同。 |
| `NORM-TS-SYNTAX`、`NORM-TS-TYPECHECK` | 查所报文件和行、TS 编号及源码合同。不要按编译器建议引入宿主类型库，也不要用注释关掉检查。 |
| `NORM-TS-ANY`、`NORM-TS-CHECK-DIRECTIVE` | 这是执行前的准入拒绝。核对真实类型和声明来源；源码用 `unknown` 时保留原来的运行时检查和失败路径。 |
| `NORM-OUT-OF-SOURCE` / `ABORT` | 分析实际走到一个既无可执行源码、又无内置定义或合法外部合同的调用。保留最小复现，不删除这次调用、不接受部分 PERS。 |
| `NORM-ADAPTER-UNSUPPORTED` | 适配没能构建执行表示。合规输入请保留并报告，不能据此断言代码已经执行到了该处。 |
| `NORM-ANALYSIS-INCOMPLETE` | 分析未完成。记录入口、规模和预算；不要截断输入。 |
| 结果、进程或引擎故障 | 保存诊断与原入口报修；若提到内部载体，提交原 TypeScript 而不是临时 Java。 |

自动化可把 `--diagnostic-format=json` 放在命令前。失败时读取 stderr 的一个 `normalizer.diagnostic/1` 对象，按 `code`、`kind`、`stage` 和可用的 `location` 决策，不匹配英文描述。用法错误退出 2，分析未成功退出 1。进程被外部终止时可能来不及返回诊断，仍不能接受 stdout。更多见[诊断架构](../implementation/docs/cli-diagnostics.md)。
