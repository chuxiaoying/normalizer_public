# 排查归一化错误与性能

用同一入口复现，把诊断、性能日志和 PERS 分开保存在临时目录。需要示例项目时用[最小 Java 输入](../../usage/getting-started.md)。

```sh
evidence=$(mktemp -d)
NORMALIZER_PROFILE_LOG="$evidence/profile.log" \
  implementation/tools/normalizer source "$project" 'src/HelloWorld.java#HelloWorld.addOne' \
  > "$evidence/pers.yaml" 2> "$evidence/stderr.log"
```

| 要观察 | 开关与类别 |
|---|---|
| 加载与校验 | `NORMALIZER_PROFILE_LOG` 中的 `kernel_input.stage` |
| 执行、分支与节点 | `NORMALIZER_DEBUG_CATEGORIES` 可选择 `kernel_execution.stage,kernel_execution.metrics`；使用 `kernel_execution.node` 时设置 `NORMALIZER_DEBUG_MAX_EVENTS` |
| 数学化简与缓存 | `NORMALIZER_MATH_PROFILE_LOG`、`NORMALIZER_MATH_PROFILE_SLOW_MS`；缓存回收可查 `math.cache.evict` |

执行日志带 `schema=normalizer.profile/1`、`run_id`、序号、时间戳和类别，可按 `run_id` 关联。性能比较固定源码提交、输入、构建模式和工具链，先构建再计时；慢输入要有相邻规模，不能只比较一次耗时。

结构深度超限先看加载阶段；活跃调用深度、节点工作量和化简固定点看执行阶段。低证明预算可以让某个捷径不成立，高预算只用于对照定位。缓存回收只允许重算，不能改变 PERS。预算耗尽而未得到完整证明时，不应发布半份正常结果。

按[维护路由](agent-maintenance.md)和[内核代码地图](kernel-execution.md)找到首次偏差，再用独立完整预期复验。日志和运行计数不代替[正式回归](../../tests/README.md)，也不进入 Git。
