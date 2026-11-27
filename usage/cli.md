# CLI 参考

在 Normalizer 仓库根目录运行以下命令。先按[新手入门](getting-started.md)装好工具，再按[AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md)准备不能直接分析的原工程。CLI 只读取交付物，不调用 AI。

| 准备好的输入 | 命令 |
|---|---|
| Java 源码工程 | `implementation/tools/normalizer source PROJECT_ROOT 'src/Entry.java#Entry.run'` |
| 独立 TypeScript 模块 | `implementation/tools/normalizer module PROJECT_ROOT --entry 'src/module.ts#exportedFunction'` |
| Vue 原组件与 TypeScript 分析模块 | `implementation/tools/normalizer scenario SCENE PROJECT_ROOT --entry 'src/page.ts#exportedFunction' --source src/page.vue` |
| 本仓正式测试 | `implementation/tools/normalizer test business-examples`，见[测试区](../tests/README.md)。 |

`PROJECT_ROOT` 指**分析工程根目录**。例如其中的文件是 `src/Entry.java`，第一个参数传工程目录，入口仍写 `src/Entry.java#Entry.run`，不要把根目录传成 `src/`，也不要在入口写绝对路径、`./` 或实际调用值。用引号包住定位符。Java 重载确有歧义才写源码参数类型，例如 `'src/Add.java#Add.run(int, int)'`；静态接收者可写 `'src/Beans.java#Beans.SERVICE.run'`。入口不是每次运行都提供一组入参的命令；CLI 分析该入口的符号输入。

### 源文件与类型检查

- Java 会读取所选工程下的 Java 源文件，再解析入口，不只读取入口所在文件。不要直接把含无关源码的生产仓库顶层当分析工程。源码必须是有效 UTF-8。源码目录中的目录软链接会被拒绝；文件软链接只能指向工程内部，定位采用提交的相对路径。
- 独立模块和 Vue 分析模块使用可执行的严格 `.ts`；不接受 `.js`、`.mjs` 或声明文件 `.d.ts`。CLI 用固定严格选项检查交付代码，不用原工程的 `tsconfig.json` 放宽规则。显式、隐式或由类型推得的 `any` 与压制检查的注释不可用。通过类型检查不等于该程序可归一化；普通无源码对象调用不能作为真实外部端口。

## Vue 场景

`--entry` 定位实际分析的 `.ts` 导出函数，`--source` 只指原始 `.vue`，两条路径都相对同一个工程。CLI 校验原件可读并保留来源，不编译 Vue、不启动应用，也不证明原件和分析模块等价。

| 场景名 | 选择时机 | 例证 |
|---|---|---|
| `frontend-vo-production` | 要看事件或生命周期入口如何读取环境并写入 VO | [事件入口](../design/examples/topics/language-entries/frontend-vo-production-vue.md) |
| `frontend-vo-consumption` | 要看某个组件属性怎样读取已有 VO 并返回观察结果 | [属性出口](../design/examples/topics/language-entries/frontend-vo-consumption-vue.md) |

直接可运行的两条完整命令见[新手入门](getting-started.md#3-vue运行生产入口与消费入口)。

## 诊断

先看退出码，再查看输出。成功时 stdout 是最终 PERS YAML；构建和诊断信息看 stderr。重定向可能预先创建空文件，文件存在不等于成功。

| 遇到的失败 | 下一步 |
|---|---|
| 构建失败、依赖缺失或命令用法错误 | 先查 Cargo、Node/npm、网络或缓存，以及命令参数顺序；尚未得到业务结果。 |
| 入口或源码校验未通过 | 核对工程根、文件扩展名和大小写、定位路径与声明、原件和分析副本是否都齐全。 |
| 诊断要求预处理或不支持 | 保留具体位置及 stderr，回到[预处理 Skill](skills/normalizer-preprocess/SKILL.md)补闭合证据；证据不够就报告缺口。 |

分析拒绝没有成功 PERS。如果合法源码可能抛出业务异常，它仍可以成功归一化，并在 PERS 中出现 `throws`。区分两类结果见[第七章](../design/chapters/第七章%20·%20PERS%20登场.md)。
