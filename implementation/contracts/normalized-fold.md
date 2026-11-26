# 遍历与聚合

业务循环应保留数据来源、初值、每次更新、过滤条件和退出条件。预处理可用有定义的关系运算表达求和和首项查询。有限重试需由源码提供语义上界，并保留每次调用和异常。

| 目的 | 可运行示例 |
|---|---|
| 从初值对行求和 | [按行累积](../../design/examples/topics/loops-and-termination/java-loop-fold.md) |
| 找到首项后停止 | [提前退出](../../design/examples/topics/loops-and-termination/java-loop-early-exit.md) |
| 外部调用有界重试 | [重试和异常](../../design/examples/topics/loops-and-termination/java-loop-bounded-retry.md) |
| 无法从来源证明终止或给出等待合同 | [无界轮询的预期诊断](../../design/examples/topics/loops-and-termination/java-loop-unknown.md) |

固定向量可按元素执行，真实的外部操作计入各自的 S 和完成值。未知长度的递推如不能投影为受支持的业务表达式，应显式诊断。不能把测试样本长度写成算法上限。
