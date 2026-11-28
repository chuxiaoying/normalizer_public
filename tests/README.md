# 从种子到完整 CLI 验收

从仓库根目录运行：

```sh
implementation/tools/normalizer test business-examples
```

这是正式业务回归，也是唯一的业务覆盖率入口。它按[正式清单](e2e/catalog.yaml)执行种子的原例和变体。`pending` 表示尚未派生，不是执行时跳过。构建与采集覆盖的命令见[验证入口](../implementation/docs/validation.md)。

## 定向复现

正式入口会准备前端依赖和内核后运行完整清单，不接收筛选参数。若只需重复某个用例，可在已有一次正式引导运行的检出里使用 Node 的名称筛选；测试体仍调用同一生产 CLI：

```sh
node --test --test-name-pattern='^control-early-return / Early\.choose / original$' \
  tests/e2e/business_examples.test.mjs
```

用例名由「种子 ID / 清单入口 / 用例目录名」组成。确认结果实际执行了目标用例；未命中或定向通过都不能充当完整回归或覆盖证据。

## 登记及文件布局

`e2e/` 顶层只有正式[测试入口](e2e/business_examples.test.mjs)、[登记清单](e2e/catalog.yaml)、用例树 `cases/`和测试支撑代码 `harness/`。支撑代码负责把种子登记对应到工程目录、加载用例，以及校验成功和拒绝输出，不另开测试入口。用例树不放说明页、日志或未登记的材料；发现额外路径会报错。

先按[种子目录](../design/examples/README.md)登记稳定 ID。测试沿用种子文件的主题路径：将 `design/examples/` 换为 `tests/e2e/cases/`，将种子文件名换为种子 ID。种子所在主题只由[种子清单](../design/examples/seeds.yaml)决定；[CLI 清单](e2e/catalog.yaml)只按 ID 登记运行方式和入口。同一种子可以登记不同语言或场景，无需复制主题目录。

种子下依次是入口、原例和变体。入口目录由[登记加载器](e2e/harness/catalog.mjs)推导：Java 使用调用名，独立 TypeScript 将定位符里的 `#` 写作 `--`，前端场景用「场景名--入口名」区分。同一入口有一个 `original/`，其余以变体名称组织。参见[早返回原例](e2e/cases/topics/control-flow/control-early-return/Early.choose/original/case.yaml)和[取负变体](e2e/cases/topics/control-flow/control-early-return/Early.choose/select-negative-fallback/case.yaml)。

| 字段 | 内容 |
|---|---|
| `entry` | 分析工程相对路径和精确调用定位。以 [CLI 的入口格式](../usage/cli.md)为准。 |
| `expected` 或 `reject` | 二选一：独立推导的完整 PERS，或 `reject.fragments` 精确核对诊断。拒绝首项必须是期望的主诊断码，后续项匹配该诊断的内容；`reject.kind` 核对诊断类别；`reject.location` 独立核对诊断中的源码位置，不依赖是否登记类别。 |

每个用例目录的 `src/` 保存真实源码与完整路径，不放框架原件；`case.yaml` 只登记上表中的入口和断言，不进入 CLI 输入。用例不保存相对原例的补丁；同一语言和场景下，入口与分析源码均相同的用例不可重复登记。执行器把 `src/` 原样复制到独立临时工程，用[正式 CLI](../usage/cli.md)运行，不从种子 Markdown 拼接源码、不重放预处理补丁，也不调用 AI。成功时先验证结构，再逐字段比较完整 PERS；拒绝时核对非零退出、空 stdout 和具体诊断。

## Java 原生语法覆盖

[种子区的语法索引](../design/examples/java-syntax-coverage.yaml)通过「种子 ID / Java 清单入口 / 用例目录」引用这里已有的正式用例。运行正式回归时先校验每条引用及其预期类型：漏登记、删错目录、把成功断言改为拒绝，都会报错。索引不增加执行次数，不决定哪些用例执行，也不代表上次运行的通过状态。报告时把索引登记数、正式回归通过数和代码覆盖率分开。

## 前端种子与测试的边界

原始模板及改写对照在设计种子里；运行用例只保存纯净 TypeScript 和独立断言。每个变体仍须能说明相对种子改了哪项观察或准入条件；不能靠原件差异区分相同的 CLI 输入。

## 如何写预期

从[设计章节](../design/README.md)、源码和真实观察范围独立推导。[统一定形](../design/chapters/08-统一定形.md)裁决唯一表达形式，含 [LET 的整值比较](../design/chapters/08-统一定形.md#cf-09-let-的整值比较)。等价变体仍须写出同一份完整预期；不从 actual 反填、不规范化实际值后再验收、不省略真实异常或效果。测试区只管理可复验的种子派生用例，不存单次审核和运行记录。

## 从种子补齐测试

1. **验一个规则。** 从最小原例写完整分析工程和独立推导的完整 PERS 或拒绝。正式输入不读取种子的 Markdown 作为预期值。
2. **验它能区分。** 等价写法保持入口合同并使用同一预期；把一个前提改成不成立，单独断言本来应该变化的结果、效果或拒绝。关键交互要有对照，比如「复用旧行」与「写后重新读取」，不能只断言两边都满足的性质。
3. **验失败与修复。** 先用正式 CLI 重现已知缺口；计划或尚未登记的失败复现不计入覆盖。修复后以同一份独立预期回验，再运行完整正式回归。不把已知错误输出或错误拒绝收进清单换绿灯。
4. **分开报告。** [种子分母计划](../design/examples/seed-plan.yaml)回答哪些语义场景已有种子；正式回归回答登记了哪些断言、执行结果怎样。covered 表示种子已有登记用例，不代表它涉及的全部前提和组合都已覆盖。
