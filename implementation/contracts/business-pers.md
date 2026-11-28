# 最终 PERS 的机器接口

本页只定位实现和校验入口。P、E、R、S、异常以及定形的设计含义，以[PERS登场](../../design/chapters/07-PERS登场.md)、[统一定形](../../design/chapters/08-统一定形.md)的章内规则为准。

| 责任 | 实现入口 |
|---|---|
| 校验 YAML 文档结构、字段、值类型写法 | [机器 schema](pers/schema.yaml)，是工具实际校验的结构，不是新的业务语义。 |
| 生成入口、参数、环境与正常结果 | `pipeline/src/kernel_input/business_entry.rs` |
| 发布有序调用、写入、SQL 和完成绑定 | `pipeline/src/kernel_input/business_event.rs`、`business_sql.rs`、`port_completion.rs` |
| 渲染表达式作用域内的成员选择及其绑定 | 同目录的 `member_choice.rs`、`temporal_graph.rs` |
| 固定格式版本和区分最终产物与调试表示 | `pipeline/src/kernel_result.rs` |

`pipeline/` 相对 `implementation/packages/normalizer/crates/`。对接方用 YAML 读取结果；公式保持表达式文本，不由 YAML 库执行。正式回归会在同一次用例执行中先校验预期和实际的 PERS 结构，再比较完整内容，见[测试入口](../../tests/README.md)。变更先核对设计；若仅调整序列化，仍须证明完整输出没有失去来源绑定、观察顺序或完成域。不在这里维护第二套 YAML 示例。

## 动态环境值的当前字段

以下字段描述当前机器接口，不新增设计语义：

- 运行中取得的环境值用 `$a_i` 表示，动作的完成绑定用 `$e_i` 表示。动态 E 用 `kind: acquisition`，有条件取得时以 `when` 表示该条件。
- 时钟取得用 `operation: normalizer.intrinsics.N.currentTimeMillis` 与 `type: number`。文档同时有 S 时，`afterS` 给出读取之前的已发布 S 前缀长度；前缀里的条件动作不因此变成无条件。没有 S 则不发布该字段。
- 同一张表再次物理读取时，以 `source` 引用原表声明。若读取依赖本次执行中对同表的成功写入，`after` 记录那些完成绑定和适用条件；没有这样的写入时省略 `after`，并不把两次读取合为一次。对已经取得的行再做计算，不新增读取。关系公式用 `FROM $a_i AS 表名` 引用新快照。语义依据见[关系读取的快照边界](../../design/chapters/05-关系代数.md#ra-08-取得后的化简不搬动读写)。

## 首项观察的机器表示

对直接取得的有序标量序列 `values`，当前公式把统一后的首项观察序列化为 `FIRST(value FROM values)`。有投影时使用实际的行值表达式，不把已取得的投影改写成底层值。这是取值公式的拼写，不能代替 `throws`、空集缺省或 `S` 中实际发生的动作；适用合同由[关系代数](../../design/chapters/05-关系代数.md#第零项何时就是首项)决定。
