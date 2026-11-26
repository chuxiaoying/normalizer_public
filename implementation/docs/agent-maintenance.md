# Agent 维护路由

这份文档把常见问题映射到唯一的责任层，减少 agent 在错误模块里堆补丁。agent 先选择场景入口；场景层负责 AI 预处理、入口适配和统一 IR 交付，归一化内核只接收已经闭合的统一 IR，最终产物必须是完整 PERS。测试 expected 由源码、合同和独立观察推导，不能从 actual 反填。

## 面向 AI 的命名和维护原则

关键元素的名称必须表达真实职责：`scenario` 表示原始业务场景入口，`kernel-input` 表示已经闭合并交给内核的输入，`contracts` 表示稳定约束，`evidence` 表示运行证据，`legacy` 或“退役”只表示追溯材料。不要用产品名替代输入载体名，也不要用 `attack` 把难度递增的回归样本误命名成攻击实现。改名时同时更新 README、脚本、schema 中的路径和证据引用，避免目录名与内容职责分离。

为降低 AI 修改时的误判，保持“一文件一职责”和“一入口一合同”：脚本只负责一个可复核的观察，fixture 只表达一条主要语义差异，expected 只保留一个独立推导的权威结果。遇到失败先沿阶段和责任层定位，不能把目录整理、命名修正或 formatter 特判当作业务语义修复；删除兼容衔接前先确认没有活动入口引用，历史材料则保留清晰的退役标记。

## 变更路由

| 现象 | 首先检查 | 责任层 | 不应直接修改 |
|---|---|---|---|
| Maven/JAR、`node_modules`、未解析导入或依赖缺失 | 预处理 skill、依赖材料和预处理产物 | AI 预处理和语言工具 | `math`、`pipeline` 的语义规则 |
| Java/前端语法无法进入统一输入 | `simulator-parser`、`implementation/tools/frontend`、语言测试 | 语言解析和预处理 | PERS formatter |
| 已闭合输入无法解析或进入内核 | `simulator-parser`、`pipeline::preprocess` | 语法边界和预处理合同 | fixture expected |
| 已闭合输入的状态、别名、异常或端口关系错误 | `pipeline::kernel_input`、`kernel_execution`、`realtime_state` | 实时状态和完成通道 | 文本格式化特判 |
| 数学等价式、CASE、关系集合或定序错误 | `math` | 数学归一化和唯一顺序 | 测试用例的偶然表达顺序 |
| R/E/T/S 内容不完整、PERS 不是唯一形式 | `pipeline`、PERS 合同测试 | 最终 PERS 产物 | 内核调试输出作为替代 expected |

## 每次改动的最小证据链

1. 记录基线提交、输入 fixture、入口、命令和失败阶段。
2. 用最小定向测试复现，并区分日志直接证明的事实、推断和未知项。
3. 在责任层修改，保留一个独立 expected 或明确拒绝合同。
4. 运行同一复现命令，确认失败消失并检查关联 PERS、异常、身份和顺序。
5. 运行与变更对应的构建、CLI 或结构检查。形成完整阶段后提交独立 commit。

## 证据入口

```sh
python3 -B implementation/tools/architecture/health.py --json
cargo fmt --manifest-path implementation/packages/normalizer/Cargo.toml --all -- --check
git diff --cached --check
```


## 维护边界

- `expected.yaml`、`behavior-expected.yaml` 和拒绝合同分别表达不同责任，不能互相替代。
- 业务逻辑优先保留，JDK、Maven、框架内部实现和资源细节沿 AI 预处理或原生边界处理。
- 大文件拆分必须先证明职责边界和私有依赖关系，再以可回退的小提交推进。只为降低行数而拆分会增加 agent 的跨文件推理成本。
- 发现“修好 A 又破坏 B”时暂停局部补丁，回到统一正则、状态域或预处理闭合条件，先重新确定共同规则。
