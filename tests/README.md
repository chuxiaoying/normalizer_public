# 从设计种子到 CLI 验收

从仓库根运行整套验收。需要 Cargo、Node.js 24 或更高版本和 npm。入口会按锁文件准备依赖，首次运行需要可用的包缓存或网络。

```sh
implementation/tools/normalizer test business-examples
```

当前执行范围由[正式测试清单](e2e/catalog.yaml)的 `groups` 定义。每个入口先运行原例，再分别运行自己的派生变体。未派生的设计种子明确列在 `pending` 中，不是 CLI 跳过的用例。整套行为回归只以这里的测试 CLI 为准。该命令只运行测试区中能追溯到设计种子的端到端用例；覆盖率仅采集这次命令执行的自有生产代码。

## 种子和测试的双重登记

[设计种子清单](../design/examples/seeds.yaml)只负责稳定 ID 与案例路径。[种子语义清单](../design/examples/seed-manifest.yaml)负责基础主题、联合主题或基础合同的分类，以及相关主题。[种子分母计划](../design/examples/seed-plan.yaml)登记应该覆盖的全部语义场景，`planned` 条目暂时没有源码和测试。[CLI 测试清单](e2e/catalog.yaml)只负责执行入口、场景和派生状态。收集测试时会把这些资料按种子 ID 连接，并检查已实现种子都位于分母计划内。

种子与实现相互独立审核，状态和审核边界见[种子审核协作协议](e2e/seed-review-protocol.md)。

topic 表示单个基础语义能力，basic 表示常规合同和防御边界，composition 只表示专门测试多个主题交互的联合种子。一个测试依赖其他主题，不会因为依赖关系自动变成 composition。测试入口所在的 groups 是执行路径，不承担设计主题归档职责。

## 每个用例直接保存输入和断言

[早返回种子](../design/examples/topics/control-flow/java-early-return.md)的[原例](e2e/cases/java/control-flow/control-early-return/Early.choose/original/case.yaml)和[取负变体](e2e/cases/java/control-flow/control-early-return/Early.choose/select-negative-fallback/case.yaml)各自是一个独立目录。以原例为例：

```text
Early.choose/original/
├── case.yaml
└── src/
    └── Early.java
```

`src/` 保存 AI 预处理后的完整源码，使用真实扩展名，保留项目相对路径。多文件用例把所有文件放进同一个 `src/`，可保留子目录；例如[跨文件类名冲突用例](e2e/cases/java/basic-contracts/java-source-name-closure/Catalog.price/conflicting-top-level-owner-across-files-does-not-bind/case.yaml)同时包含 `Catalog.java` 和 `OtherPrice.java`。Vue 同时保留 CLI 要求的原始组件文件用于来源标记，实际分析的是预处理后的 TypeScript 模块。

`case.yaml` 只包含：

- `entry`：项目相对源码路径和入口，例如 `src/Early.java#Early.choose`。
- `vueSource`：仅 Vue 用例填写，指向用例内的原始组件文件。
- `expected`：独立推导的完整 PERS，实际结果须通过结构校验并逐字段相等。若用例按设计应拒绝，用 `reject.fragments` 及必要的诊断类别或位置代替：断言非零退出、无 PERS 和明确诊断。

用例按原有目录分组，位于 `e2e/cases/<分组>/<入口>/<用例>/`；主主题分组在入口前还有种子 ID。测试清单登记种子、入口和 Vue 场景。每个入口至少有 `original/`。新增用例应提交完整源码和独立预期，不存输入差异或预期字段补丁。相同场景、入口和实际分析源码只登记一次，不能有两份预期。

运行时原样复制整个 `src/` 到独立临时工程，再执行正式 CLI 的 Java `source`、TypeScript `module` 或 Vue `scenario`。`case.yaml` 不进入 CLI 输入。不读取种子 Markdown 来拼源码，不重放种子补丁，也不会在运行时调用 AI。设计种子仍是输入和预期的溯源依据。独立 TypeScript 入口只接收严格 `.ts`；原始 JavaScript 须先预处理成严格 TypeScript。

## 一致性复核判据

- `today` 是每次调用固定的输入；等待前后读取应相同。真正可变的宿主状态另行测试。
- `LET` 仅在被绑定值本身含 CASE WHEN 且出现超过一次时允许；外层 CASE 或重复但不含 CASE 的聚合不是理由。
- 数值复制不舍入；位数和中点模式分别由源码交付。小数的独立样例覆盖正负中点、两个显式模式及超出二进制浮点精度的进位。
- 写入用数据库语句 `S.sql`，读关系的 `R` 继续用公式语法。读写、空集、实际受影响行数和失败回执各自保留，不以变更预期掩盖执行缺陷。
