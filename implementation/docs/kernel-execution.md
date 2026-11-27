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

## 复核状态转移

先用原源码的具体值定位，再以符号输入核对定义域。给已绑定的非空引用做写前写后的独立对照：

```text
read(write(H,r,f,v),q,g)
  = CASE WHEN r=q AND f=g THEN v ELSE read(H,q,g) END
```

未知别名保留身份条件，不能用变量名比较引用。每步同时对照堆、效果轨迹和完成方式；核对前一步终止后，续体没有在该路径执行。外部调用先取得当时的实参，再登记实际调用；失败响应不能抹掉已发生的调用。检查批量或有序循环时，对照真实长度与前缀，不用一个运行样本代替全域。

## 复核出口

内部返回沿原引用继续执行。入口返回、调用参数和可观察写入各用其发生时的状态，发布阶段不重新读取后来才写入的值。只遍历边界声明的字段；检测环核对当前路径是否返回祖先，不把兄弟路径共享同一子对象判成环。缺少足够的投影证据时走诊断，不截断对象、不凭空生成编号。完整 PERS 的验收仍通过[正式回归](../../tests/README.md)。采样与日志见[排查入口](troubleshooting.md)。
