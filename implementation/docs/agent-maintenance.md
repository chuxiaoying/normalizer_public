# Agent 维护路由

这份文档把常见问题映射到唯一的责任层，减少 agent 在错误模块里堆补丁。先根据[三模块接线](architecture-three-modules.md)区分 CLI 外的 AI 预处理、CLI 内的语言适配与内核，再用下表缩小排查范围。

## 变更路由

| 现象 | 首先检查 | 责任层 | 不应直接修改 |
|---|---|---|---|
| Maven/JAR、`node_modules`、未解析导入或依赖缺失 | 预处理 skill、依赖材料和预处理产物 | AI 预处理和语言工具 | `math`、`pipeline` 的语义规则 |
| Java/前端语法无法进入统一输入 | `simulator-parser`、`implementation/tools/frontend`、同一入口的 CLI 回归 | 语言解析和预处理 | PERS formatter |
| 已闭合输入无法解析或进入内核 | `simulator-parser`、`pipeline::kernel_input` | 语法边界和预处理合同 | fixture expected |
| 已闭合输入的状态、别名、异常或端口关系错误 | `pipeline::kernel_input`、`kernel_execution`、`realtime_state` | 实时状态和完成通道 | 文本格式化特判 |
| 数学等价式、CASE、关系集合或定序错误 | `math` | 数学归一化和唯一顺序 | 测试用例的偶然表达顺序 |
| R/E/T/S 内容不完整、PERS 不是唯一形式 | `pipeline`、正式回归的 PERS 校验与比较 | 最终 PERS 产物 | 内核调试输出作为替代 expected |

## 每次改动的最小证据链

1. 记录基线提交、输入 fixture、入口、命令和失败阶段。
2. 用最小定向测试复现，并区分日志直接证明的事实、推断和未知项。
3. 在责任层修改，保留一个独立 expected 或明确拒绝合同。
4. 运行同一复现命令，确认失败消失并检查关联 PERS、异常、身份和顺序。
5. 运行与变更对应的构建、CLI 或结构检查。形成完整阶段后提交独立 commit。

命令集中在[验证入口](validation.md)。

## 维护边界

- 正式预期在用例 YAML 内，与完整输入共存；不另存 `expected.yaml` 或用内部行为产物替代。
- 大文件拆分必须先证明职责边界和私有依赖关系，再以可回退的小提交推进。只为降低行数而拆分会增加 agent 的跨文件推理成本。
- 发现“修好 A 又破坏 B”时暂停局部补丁，回到统一定形、状态域或预处理闭合条件，先重新确定共同规则。
