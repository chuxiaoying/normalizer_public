# 从设计种子到 CLI 验收

从仓库根运行整套验收。需要 Git、Cargo、Node.js 24 或更高版本和 npm。入口会按锁文件准备依赖，首次运行需要可用的包缓存或网络。

```sh
implementation/tools/normalizer test business-examples
```

当前执行范围由[正式测试清单](e2e/catalog.yaml)的 `groups` 定义。每个入口先运行原例，再分别运行自己的派生变体。未派生的设计种子明确列在 `pending` 中，不是 CLI 跳过的用例。整套行为验收只以这里的测试 CLI 为准。

## 种子和测试的双重登记

[设计种子清单](../design/examples/seeds.yaml)只负责稳定 ID 与案例路径。[种子语义清单](../design/examples/seed-manifest.yaml)负责基础主题、联合主题或基础合同的分类，以及相关主题。[种子分母计划](../design/examples/seed-plan.yaml)登记应该覆盖的全部语义场景，`planned` 条目暂时没有源码和测试。[CLI 测试清单](e2e/catalog.yaml)只负责执行入口、场景和派生状态。收集测试时会把这些资料按种子 ID 连接，并检查已实现种子都位于分母计划内。

种子与实现相互独立审核，状态和审核边界见[种子审核协作协议](e2e/seed-review-protocol.md)。

topic 表示单个基础语义能力，basic 表示常规合同和防御边界，composition 只表示专门测试多个主题交互的联合种子。一个测试依赖其他主题，不会因为依赖关系自动变成 composition。测试入口所在的 groups 是执行路径，不承担设计主题归档职责。

## 看一个种子怎样长出三条验收

[整数加一](../design/examples/topics/expression-simplification/java-static.md)规定业务数值无固定上限。它给出原始 Java、可原样交付的预处理结果，以及含 `R: value + 1` 的完整 PERS。[对应变体](e2e/variants/java/pure/HelloWorld.addOne.yaml)只记录相对种子的变化。

| 运行项 | 给 CLI 的程序 | 独立预期 |
|---|---|---|
| 原例 | `return value + 1;` | 与设计给出的完整 PERS 一致。 |
| 等价变体 | 先算 `next = value + 1`，再加上 `if (next <= value) return 99;` | 无界业务整数加一必然变大，该分支不可达。沿用原例的完整预期。 |
| 改变业务的变体 | 改成 `return value + 2;` | 将种子预期的 `R` 改为 `value + 2`，其余字段仍要逐一匹配。 |

## 如何扩展

1. **设计区登记种子。** 在[种子清单](../design/examples/seeds.yaml)登记稳定 ID 和案例路径。案例拥有原始源码、预处理交付物与独立预期。多个入口可以共用一篇，例如 SQL 的查询、插入、更新。
2. **测试区登记覆盖。** 在[测试清单](e2e/catalog.yaml)以同一 ID 登记入口，暂未派生的种子显式列入 `pending`。每个入口对应 `e2e/variants/<分组>/<入口>.yaml`；按主主题归档的分组在分组与入口之间增加种子 ID，以区别不同种子的同名入口，入口名中的 `#` 在文件名里写作 `-`。Vue 同时登记 CLI 的场景。
3. **一次改变一个有业务含义的条件。** 每个具名变体都从原例重新开始。用 `edits` 指定规范源码中唯一匹配的位置。 只有场景本身需要新增独立源码文件时，才用 `file` 和 `create` 交付完整新文件；不能覆盖种子已有文件。等价改写直接继承完整 expected；改变行为时，用 `expect` 列出应变的字段；此前没有的字段用 `add: 值` 显式新增；字段应当完全消失时，用 `absent: true` 断言不存在（例如没有发生任何外部调用时的 `S`）；应拒绝的变体用 `reject` 给出首条诊断标记、可用的错误类别和具体原因。若种子本身就要求拒绝，系列在 `variants` 外声明统一的 `reject`，同时核对种子给出的拒绝类型和入口。预期由业务关系推出，再与 CLI 实际输出比对。
4. **运行整套验收。** 收集阶段核对设计文档、种子 ID、已覆盖或待派生状态、真实入口和变体文件。漏登记、重复绑定、孤儿文件或无法唯一应用的改动都会报错。

变体可以用显式 `add` 增加此前没有的字段，用 `absent` 验证字段不出现。两者都是对完整 PERS 的独立断言，不能让同一用例接受两个结果。

每条验收创建独立临时工程，回放已审阅的 AI 预处理 diff，再从正式 CLI 的 Java `source`、TypeScript `module` 或 Vue `scenario` 入口执行。原始文件可以是 JavaScript 或 Vue；补丁前后的路径分别记录原件与分析副本，正式执行只选分析副本中的唯一定位。成功路径比较完整 PERS 结构和表达式；拒绝路径检查非零退出、空 stdout 和具体诊断。这里覆盖的是已给定预处理交付物后的端到端工程链路，AI 实时生成预处理结果的质量需单独评估。

独立 JavaScript 模块通过 `implementation/tools/normalizer module PROJECT_ROOT src/module.js exportedFunction` 运行。TypeScript 使用同一入口，指定相对 `.ts` 路径；可选槽、显式 undefined 和 rest 索引由有类型的源码与共享内核验收，不把类型声明剥离成 JS 来代替类型检查。当前入口接收带明确 JSDoc 类型的闭合同步标量函数，并从语言适配器交付共享有类型 IR；独立异步入口仍须补齐一般调用与完成语义；若入口明确直接调用普通对象入参上的无源码方法，独立负向例断言内核 Panic、具体调用目标及空 stdout。缺少能力时保留独立完整预期或精确拒绝合同，且不跳过。无源码的调用不是天然端口：不允许伪造端口让正向预期通过。

`implementation/tools/normalizer test pers-goldens` 离线检查所有正向预期（含变体）的 PERS schema、类型引用和回执绑定顺序，不启动内核。它不证明业务行为；完整 CLI 回归仍逐项比较实际 PERS 与独立预期，并在比较前检查两者结构。

`implementation/tools/normalizer test seed-patches` 回放全部登记设计种子的预处理补丁（含待派生的种子），不启动内核。补丁回放通过不等于该种子已进入正式行为回归。


`implementation/tools/normalizer test engineering` 运行架构工具测试、设计阅览器生命周期测试和文档链接检查；`implementation/tools/normalizer test kernel-units` 验证内核内部结构不变量。它们不是业务种子，不改变业务回归清单。

## 一致性复核判据

- `today` 是每次调用固定的输入；等待前后读取应相同。真正可变的宿主状态另行测试。
- `LET` 仅在被绑定值本身含 CASE WHEN 且出现超过一次时允许；外层 CASE 或重复但不含 CASE 的聚合不是理由。
- 数值复制不舍入；位数和中点模式分别由源码交付。小数的独立样例覆盖正负中点、两个显式模式及超出二进制浮点精度的进位。
- 写入用数据库语句 `S.sql`，读关系的 `R` 继续用公式语法。读写、空集、实际受影响行数和失败回执各自保留，不以变更预期掩盖执行缺陷。


### 源码入口的文件边界

`normalizer test business-examples` 同时经过正式源码 CLI 验收文件入口错误。缺失来源、空目录、读取失败、非法 UTF-8 文件名或内容都不得生成 PERS；错误应区分阶段，并在有来源文件时包含其名字。 源码遍历不跟随目录软链接；文件软链接只能指向所选源码根内，仍使用提交的链接路径作为来源。此组测试的是 CLI 读取合同，不把文件系统错误伪造为业务种子或异常记录。
