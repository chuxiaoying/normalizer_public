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

先按[种子目录](../design/examples/README.md)登记稳定 ID，再在 [CLI 清单](e2e/catalog.yaml)以同一 ID 登记入口和场景。用例位于 `e2e/cases/<分组>/<入口目录>/<用例>/`；按主主题分组时，在入口目录前加种子 ID。入口目录由[登记加载器](e2e/catalog.mjs)从清单计算，不总是入口定位符原文。每个入口有一个 `original/` 用例。参见[早返回原例](e2e/cases/java/control-flow/control-early-return/Early.choose/original/case.yaml)和[取负变体](e2e/cases/java/control-flow/control-early-return/Early.choose/select-negative-fallback/case.yaml)。

| 字段 | 内容 |
|---|---|
| `entry` | 分析工程相对路径和精确调用定位。以 [CLI 的入口格式](../usage/cli.md)为准。 |
| `vueSource` | 仅 Vue：相对于分析工程的原始 `.vue` 路径，与 `entry` 指向的 `.ts` 一起存入 `src/`。场景名在清单登记。 |
| `expected` 或 `reject` | 二选一：独立推导的完整 PERS，或 `reject.fragments` 精确核对诊断。拒绝首项必须是期望的主诊断码，后续项匹配该诊断的内容；`reject.kind` 核对诊断类别；`reject.location` 独立核对诊断中的源码位置，不依赖是否登记类别。 |

每个用例目录的 `src/` 保存真实源码与完整路径，Vue 另保留原组件；`case.yaml` 只登记上表中的入口和断言，不进入 CLI 输入。用例不保存相对原例的补丁；同一语言和场景下，入口、Vue 来源定位和分析源码均相同的用例不可重复登记；只改原始 `.vue` 内容不会构成新的分析输入。执行器把 `src/` 原样复制到独立临时工程，用[正式 CLI](../usage/cli.md)运行，不从种子 Markdown 拼接源码、不重放预处理补丁，也不调用 AI。成功时先验证结构，再逐字段比较完整 PERS；拒绝时核对非零退出、空 stdout 和具体诊断。

## Vue 原件和分析副本

同一用例的 `.vue` 与分析 `.ts` 应表达同一次入口观察。变体若改了属性结果、状态写入或取得时点，源组件须改同一处业务含义；只改不影响观察的写法时可保留原件。刻意破坏分析副本以验证拒绝的用例保留原件，并在用例名称标出所验证的无效输入。CLI 不校验两份源码等价，审核须直接对照二者。

## 如何写预期

从[一级与二级设计](../design/README.md)、源码和真实观察范围独立推导。第八章与 [LET 专题](../design/secondary/什么时候可以引入%20LET.md)裁决唯一表达形式。等价变体仍须写出同一份完整预期；不从 actual 反填、不规范化实际值后再验收、不省略真实异常或效果。测试区只管理可复验的种子派生用例，不存单次审核和运行记录。
