# CLI 参考

在 Normalizer 仓库根目录运行以下命令。先按[新手入门](getting-started.md)装好工具，再按[AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md)准备不能直接分析的原工程。CLI 只读取交付物，不调用 AI。

| 准备好的输入 | 命令 |
|---|---|
| Java 源码工程 | `implementation/tools/normalizer source PROJECT_ROOT 'src/Entry.java#Entry.run'` |
| 独立 TypeScript 模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/module.ts#exportedFunction'` |
| 前端事件或属性观察的严格 TypeScript 模块 | `implementation/tools/normalizer scenario SCENE PROJECT_ROOT --entry 'src/page.ts#exportedFunction'` |
| 本仓正式测试 | `implementation/tools/normalizer test business-examples`，见[测试区](../tests/README.md)。 |

`PROJECT_ROOT` 指**分析工程根目录**。例如其中的文件是 `src/Entry.java`，第一个参数传工程目录，入口仍写 `src/Entry.java#Entry.run`，不要把根目录传成 `src/`，也不要在入口写绝对路径、`./` 或实际调用值。用引号包住定位符。Java 重载确有歧义才写源码参数类型，例如 `'src/Add.java#Add.run(int, int)'`；静态接收者可写 `'src/Beans.java#Beans.SERVICE.run'`。入口不是每次运行都提供一组入参的命令；CLI 分析该入口的符号输入。

### 源文件与类型检查

- Java 会读取所选工程下的 Java 源文件，再解析入口，不只读取入口所在文件。只有生产源码已经满足纯净、闭合和有界的分析要求时，才适合原样交付。不要直接传含无关源码的生产仓库顶层，也不要为了逐文件通过校验而删除会影响入口的装配、初始化或失败路径。源码必须是有效 UTF-8。源码目录中的目录软链接会被拒绝；文件软链接只能指向工程内部，定位采用提交的相对路径。
- 独立模块和前端场景模块使用可执行的严格 `.ts`；不接受 `.js`、`.mjs` 或声明文件 `.d.ts`。显式、隐式或由类型推得的 `any` 与压制检查的注释不可用。通过类型检查不保证该程序可归一化。

### 当前实现限制

这些是当前仓库 CLI 的限制，不改变[分析副本校验](../design/secondary/分析副本怎样通过校验.md)和[标准操作](../design/secondary/标准操作与声明标记.md)的设计合同。

- **TypeScript：** 当前只检查单个分析模块，使用固定严格选项，不读原工程的 `tsconfig.json`，也不支持跨文件模块解析。只有能证明不改变绑定、初始化、执行顺序和观察行为时，才为当前 CLI 整合成单文件；否则报告适配缺口，不要求设计上的纯净工程一律放在单文件。检查整份模块，包括没调用的辅助函数，并拒绝导入，包括设计中的官方 `@normalizer/stdlib`。入口必须是文件内有函数体的具名导出函数声明，例如 `export function run(...)`；当前不接受 `#ops.run` 这样的导出对象方法，也不接受导出的箭头函数变量作为入口。
- **环境读取：** 当前 CLI 尚未交付 `N.currentTimeMillis()`。如果分析依赖执行过程中取得的时钟值，按[预处理 Skill](skills/normalizer-preprocess/SKILL.md)报告该缺口，不以常量、新入参或外部写入替代。
- **同表写后重读：** 当前 CLI 不能可靠区分写入前后取得的表数据。遇到依赖两次观察的输入，即使成功输出也不要当作设计合规结果；保留原有读写次序并报告缺口。[关系指令的统一写法](../design/secondary/关系指令的统一写法.md)规定写后重读是新的观察。

## 前端 VO 场景

`--entry` 定位分析工程内的 TypeScript 导出函数。场景名选择事件或属性的观察方式，不选择 UI 框架。原始 `.vue`、`.axml`、组件配置和改写对照留在预处理侧作为审查证据；不交给 CLI，也不靠 CLI 证明预处理等价。

| 场景名 | 选择时机 | 例证 |
|---|---|---|
| `frontend-vo-production` | 看事件或生命周期入口（如 `onLoad`）如何读取环境并写入页面展示状态（VO） | [事件入口](../design/examples/topics/language-entries/frontend-vo-production-vue.md) |
| `frontend-vo-consumption` | 看单个组件属性如何读已有 VO 并返回展示值；准备成同步、导出的无参函数，不分析整次渲染 | [属性出口](../design/examples/topics/language-entries/frontend-vo-consumption-vue.md) |

**当前场景适配限制，不是设计约束：** 生产适配器目前只接收具名的 `async` 导出入口，能处理的等待写法包括直接等待已声明的外部端口。第七章里的同步生产事件仍是合法设计输入，不能加一个 `async`、把未等待的动作改成等待，或把并发等待改成串行，只为通过当前适配器。如果没有能维持原完成合同和观察结果的表示，就报告工具缺口。

直接可运行的两条完整命令见[新手入门](getting-started.md#3-前端-vo运行生产入口与消费入口)。

## 诊断

先看退出码，再核对输出。退出码为 0 不证明分析副本忠实于原工程，也不证明已绕开的实现缺口不影响观察。三个分析命令成功时，标准输出（stdout）是最终 PERS YAML；构建和诊断信息看标准错误（stderr）。`test business-examples` 输出的是测试报告。重定向可能预先创建空文件，文件存在不等于成功。

| 遇到的失败 | 下一步 |
|---|---|
| 构建失败、依赖缺失或命令用法错误 | 先查 Cargo、Node/npm、网络或缓存，以及命令参数顺序；尚未得到业务结果。 |
| 入口或源码校验未通过 | 只核对分析工程的根目录、源码扩展名和大小写、入口定位和所需声明。不要为排错把原始框架文件加入工程。 |
| 分析未完成或预算不足 | 按未完成处理，不把局部输出当最终结果，也不凭空缩小循环或取数范围。 |
| 诊断要求预处理或不支持 | 保留具体位置及 stderr，按[预处理 Skill](skills/normalizer-preprocess/SKILL.md)区分输入证据缺失和当前实现不支持。前者补证据，后者如实报告，不改业务行为以换取成功。 前端错误若提到内部工程或 Java，不表示该把 TypeScript 改写成 Java；先保留纯净 TypeScript 的最小复现。 |

分析拒绝没有成功 PERS。如果合法源码可能抛出业务异常，它仍可以成功归一化，并在 PERS 中出现 `throws`。区分两类结果见[第七章](../design/chapters/第七章%20·%20PERS%20登场.md)。
