# 从种子到完整 CLI 验收

从仓库根目录运行：

```sh
implementation/tools/normalizer test business-examples
```

这是正式业务回归，也是唯一的业务覆盖率入口。它按[正式清单](e2e/catalog.yaml)执行种子的原例和变体。`pending` 表示尚未派生，不是执行时跳过。构建与采集覆盖的命令见[验证入口](../implementation/docs/validation.md)。

## 登记及文件布局

先按[种子目录](../design/examples/README.md)登记稳定 ID，再在 [CLI 清单](e2e/catalog.yaml)以同一 ID 登记入口和场景。用例位于 `e2e/cases/<分组>/<入口>/<用例>/`；主主题分组在入口前加种子 ID。每个入口有一个 `original/` 用例。参见[早返回原例](e2e/cases/java/control-flow/control-early-return/Early.choose/original/case.yaml)和[取负变体](e2e/cases/java/control-flow/control-early-return/Early.choose/select-negative-fallback/case.yaml)。

| 字段 | 内容 |
|---|---|
| `entry` | 项目相对路径和精确调用定位；Vue 另有 `vueSource` 指向原组件。 |
| `expected` 或 `reject` | 二选一：独立推导的完整 PERS，或 `reject.fragments` 与必要的诊断类别、位置。 |

每个用例目录的 `src/` 保存真实源码与完整路径，Vue 另保留原组件；`case.yaml` 只登记上表中的入口和断言，不进入 CLI 输入。用例不保存相对原例的补丁；相同场景、入口与分析源码不得重复登记。执行器把 `src/` 原样复制到独立临时工程，用[正式 CLI](../usage/cli.md)运行，不从种子 Markdown 拼接源码、不重放预处理补丁，也不调用 AI。成功时先验证结构，再逐字段比较完整 PERS；拒绝时核对非零退出、空 stdout 和具体诊断。

## 如何写预期

从[一级与二级设计](../design/README.md)、源码和真实观察范围独立推导。第八章与 [LET 专题](../design/secondary/什么时候可以引入%20LET.md)裁决唯一表达形式。等价变体仍须写出同一份完整预期；不从 actual 反填、不规范化实际值后再验收、不省略真实异常或效果。测试区只管理可复验的种子派生用例，不存单次审核和运行记录。
