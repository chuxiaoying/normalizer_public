# 种子与 Normalizer 重构协作协议

种子设计和 Normalizer 工程按批次协作。设计方提供完整种子与测试，重构方负责独立挑战输入合同、PERS expected 和工程可执行性。双方通过对话沟通结论，用 Git 提交保存每一轮可复核的产物。

## 一轮协作的状态

```text
proposed
  → challenged
  → revised
  → accepted
  → cli-passed
  → merged
```

- `proposed` 表示种子和测试已经提交，测试输入是 AI 预处理后的源码。
- `challenged` 表示重构方指出输入、断言、规范或工程能力方面的具体问题。
- `revised` 表示设计方接受挑战并修改种子或测试，或重构方接受原方案并说明理由。
- `accepted` 表示双方对源码输入、观察范围、PERS expected 和测试变化达成一致。
- `cli-passed` 表示重构方使用正式 CLI 入口实跑通过，记录通过数量和失败诊断。
- `merged` 表示主 agent 审核 diff、独立预期和 CLI 结果后，把本轮提交推到 `master`。

## 每轮交付物

设计方提交以下内容：

1. 设计种子，包含原始代码、AI 预处理结果和理想 PERS。
2. `seeds.yaml`、`seed-manifest.yaml` 和 `seed-plan.yaml` 的登记。
3. 测试目录中的原例入口和派生变体。
4. 每个变体的源码编辑和独立 expected 或拒绝断言。

重构方审核以下内容：

1. 测试送入工程的确实是 AI 预处理后的源码，测试没有把 AI 预处理能力混入工程验收。
2. 预处理后的源码仍符合目标语言 parser 的输入合同，Normalizer 标记的扩展语义边界清楚。
3. PERS expected 与源码、PERS schema、统一正则规范和业务数值域一致。
4. 变体一次只改变一个可说明的业务条件，断言覆盖实际受影响的 P、E、S、T、R 或 throws。
5. CLI 失败是工程缺陷还是用例缺陷，诊断必须能指向具体一方。

## 提交与推送

每轮使用独立中文提交，提交正文写清种子范围、审核结论和 CLI 结果。重构方可以提交工程修复或审核记录，但不直接推送 `master`。只有双方在对话中明确标记 `accepted` 和 `cli-passed`，主 agent 才检查完整 diff 后推送 `master`。

未达成一致的批次保留在工作分支或审核提交中，不能通过把案例列为 pending、放宽 expected 或删除失败路径来制造通过。

## 本轮批次标识

首轮批次是 `seed-batch-1`，包含五个种子和五个 Java 入口。当前测试变体位于 `tests/e2e/variants/java/seed-batch-1/`。集合筛选案例的局部 IR 语法需要重构方重点审核，不能因为其他四个原生 Java 案例可运行就自动视为通过。
