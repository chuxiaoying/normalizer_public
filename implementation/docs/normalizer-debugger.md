# 定位在线归一化耗时和错误

从触发问题的源码目录或规范工程出发，用同一入口复现。将诊断和最终输出分别保存在临时目录，便于检查成功、失败和守卫条件。

```sh
evidence=$(mktemp -d)
NORMALIZER_PROFILE_LOG="$evidence/profile.log" \
  implementation/tools/normalizer source "$project" HelloWorld.addOne \
  > "$evidence/pers.yaml" 2> "$evidence/stderr.log"
```

示例中的源目录变量可用[最小 Java 实例](../../usage/getting-started.md)创建。`module` 或 `scenario` 的故障也使用原正式入口复现。

`kernel_input.stage` 记录工程解析及校验，`kernel_execution.stage` 描述入口执行，`kernel_execution.metrics` 记录分支拆分、函数调用、节点访问和深度等计数。复现失败时保留 stderr 及退出码。比较性能时固定输入、构建模式和重复次数，区分首次编译用时与执行用时。

需要定位某一类执行事件时，设置 `NORMALIZER_DEBUG_CATEGORIES`，例如 `kernel_execution.stage,kernel_execution.metrics`。节点级采样使用 `kernel_execution.node`，必须同时设置 `NORMALIZER_DEBUG_MAX_EVENTS` 限制输出量。日志行带有 `schema=normalizer.profile/1`、`run_id`、序号、时间戳和类别，多个阶段的记录可按 `run_id` 合并。

当最终 PERS 不符时，先判断错误首次出现在预处理、来源绑定、执行状态还是出口投影。检查事件是否在正确的路径、正确的时点取得，以及后续值是否引用同一次调用的完成符号。完整断言与复现命令见[测试架构](../../tests/README.md)。
