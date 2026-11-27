# 内核执行、状态与出口排查

本页供内核维护者确定问题首次出现的位置。执行与输出的语义分别见[第一章](../../design/chapters/第一章%20·%20JVM%20模拟器：执行即归约.md)、[第二章](../../design/chapters/第二章%20·%20多重宇宙的分裂坍缩.md)和[第七章](../../design/chapters/第七章%20·%20PERS%20登场.md)。接口字段查[机器接口](../contracts/business-pers.md)。

## 按执行阶段定位

路径相对 `implementation/packages/normalizer/crates/pipeline/src/`。

| 首次偏差 | 首查文件 | 对照证据 |
|---|---|---|
| 入口绑定、声明或加载失败 | `kernel_input.rs`、`kernel_input/source_entry.rs` | [源码交接](../contracts/normalized-source-contract.md) |
| 值、控制流、循环或执行预算 | `kernel_execution.rs`、`kernel_input/source_control.rs` | 发生条件、执行前缀和完成方式 |
| 对象、别名、堆更新或完成状态 | `runtime_heap.rs`、`realtime_state.rs` | [别名与有序写入](../../design/examples/topics/state-and-aliasing/java-state-alias.md) |
| 外部事件、回执或顺序 | `kernel_input/business_event.rs`、`kernel_input/port_completion.rs` | [有界重试](../../design/examples/topics/loops-and-termination/java-loop-bounded-retry.md) |
| 入口返回或出口内容 | `kernel_input/business_entry.rs`、`kernel_input/source_objects.rs` | [环与无环的出口](../../design/examples/topics/state-and-aliasing/java-cycle-observable.md) |

## 怎样缩小偏差

1. 对同一个入口先复现具体值，再用符号输入检查别名、分支条件和完成路径。对照[身份与稳定编号](../../design/secondary/观察身份与稳定编号.md)与表中的种子，不从调试输出反推规则。
2. 对照首次分歧时的堆、执行轨迹及最终发布：如果状态正确但输出不对，先查出口投影；如果动作已在轨迹里丢失，回查执行，不改序列化补救。外部结果和异常按[完成合同](../../design/secondary/回执与异常怎样进入%20PERS.md)核对。
3. 用[诊断与采样](troubleshooting.md)定位，再用[正式回归](../../tests/README.md)验收完整输出。

## 定形排查的实现边界

- **入口观察编号。** 单一入口对象且取得条件相同，是一类可以独立检查的入口快照。排查时可从入口对象类型和 `heap_fields` 找到字段来源，再核对公开引用是否一起重命名。带调用和 T 的混合依赖不能仅凭这类用例宣称已完成全图编号。
- **返回守卫。** 用宿主提供的 `SortContext` 核对结构键；把原生 Boolean／null 比较和真正由已取得响应声明认证的字段，分别作为排查入口。实现眼下能认证哪些表达式，不缩小[设计规定的证明义务](../../design/secondary/CASE%20的取得域与等价化简.md)。
- **异常呈现。** `kernel_input/business_event.rs` 当前按异常类聚合同类无载荷结果；有载荷时先核对终态互斥，再使用类名、载荷结构及条件结构作为排序键。这是当前发布器的行为，不替代第七、八章对异常观察和定形的要求。
