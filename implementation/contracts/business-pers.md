# 最终 PERS 的机器接口

本页只定位实现和校验入口。P、E、R、S、T、异常以及定形的含义，以[第七章《PERS 登场》](../../design/chapters/第七章%20·%20PERS%20登场.md)、[第八章《统一定形规范》](../../design/chapters/第八章%20·%20统一定形规范.md)及其二级专题为准。

| 责任 | 实现入口 |
|---|---|
| 校验 YAML 文档结构、字段、值类型写法 | [机器 schema](pers/schema.yaml)，是工具实际校验的结构，不是新的业务语义。 |
| 生成入口、参数、环境与正常结果 | `pipeline/src/kernel_input/business_entry.rs` |
| 发布有序调用、写入、SQL 和完成绑定 | `pipeline/src/kernel_input/business_event.rs`、`business_sql.rs`、`port_completion.rs` |
| 发布纯递推和成员选择的已绑定定义 | 同目录的 `pure_fold.rs`、`member_choice.rs`、`temporal_graph.rs` |
| 固定格式版本和区分最终产物与调试表示 | `pipeline/src/kernel_result.rs` |

`pipeline/` 相对 `implementation/packages/normalizer/crates/`。对接方用 YAML 读取结果；公式保持表达式文本，不由 YAML 库执行。正式回归会在同一次用例执行中先校验预期和实际的 PERS 结构，再比较完整内容，见[测试入口](../../tests/README.md)。变更先核对设计；若仅调整序列化，仍须证明完整输出没有失去来源绑定、观察顺序或完成域。不在这里维护第二套 YAML 示例。
