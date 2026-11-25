# 状态观察的最终 PERS

VO／业务对象的入口读取是 E，出口可观察字段值是 S。`normalized_pers_state` 将已验证的有序语义归约为纯值关系；输出不含 load/store/if/return 程序，也不把内部写入日志改名放进 T。

模拟 heap 的身份与别名和业务出口的内容是两个层次。分析入口最终 R、RPC 参数和其他 S 按各自合同在对应时点投影内容；内部函数返回只传递模拟值或引用。普通内容输出不公开新对象身份，也不把其初始化字段变成 S。统一类型表示和特殊边界见[出口内容投影](../boundary-value-projection.md)。下文涉及分配逃逸的拒绝描述是当前实现限制，不是目标输出必须保留新身份的要求。

```yaml
E:
  - id: $e_0
    kind: state
    target: state
    field: accepted
    at: entry
    cond: state != null
R: NOT ($e_0)
T: []
S:
  - kind: state
    target: state
    field: accepted
    cond: state != null
    value: NOT ($e_0)
  - kind: state
    target: state
    field: receipt
    cond: state != null
    value: 'false'
throws:
  - exception: java.lang.NullPointerException
    cond: state == null
    payload: 'null'
```

这个 React snapshot 示例读取一次入口 accepted，两次同值内部更新仅形成一个出口字段结果。updater 的两次取反使 accepted 恢复入口值，因此该字段不进入出口 S；receipt=false 仍保留。E 仅列被最终结果实际引用的入口观察，没有观察时省略整个 E。

`target` 表达对象身份，`field` 表达该对象的业务字段。路径拼写不同不证明对象不同；未知别名以身份相等的 CASE 关系处理。输入对象的不可变字段快照中选出的引用仍可指向业务对象，例如 map.get(key)。正常返回或继续传播的单一局部分配对象只能投影为有限、无环的字段内容；投影不公开新身份，也不把未知 heap 编号写入 PERS。已经确定抛出的完成域可以保留此前外部状态写回的内容，但不输出该局部身份作为返回引用。多个局部身份、集合/循环分配、自环、循环内容或无法证明有限内容的分配身份仍明确失败。

S 的条件是该字段值成立的完成域，包含正常和异常完成。对于 `map.get(key).value=7; return alias.value`，如果最后 alias=null，异常路径仍保留此前 value=7，不能当作自动回滚。异常载荷保留在 throws；同类型、同载荷的条件可以合并。

R 沿用已有的正常返回域惯例：throws 表示的路径不需要人为补 R:null。只有证明全部完成路径抛出且 throws 覆盖全集时，无正常值用 `R: ⊥`；文本字面量“⊥”仍由表达式格式器带引号显示，两者不同。

当前公共输出支持同步的字段读写、分支、词法 scope、catch、return/throw、可消去的局部 allocation，以及单一局部对象的有限无环内容投影。所有值运算复用统一闭合 FuncKey 白名单；仅标记 pure 的未知黑盒不是完整 PERS。入口字段读取和此前字段写入可以与共享纯 fold 递推组合。固定入口字段的循环写入、单字段按需首读、条件分支及 break/continue/方法返回已用完整类型的纯 record 递推表达；异常以前的字段写入仍保留。有限 Vector 上固定字段写入与同步只读端口调用已可用纯状态历史和索引调用族表达，格式与因果要求见 [final-state-history-pers.md](final-state-history-pers.md)。Source 收集、多字段按需获取及其与循环调用的组合、task、多个局部身份、自环或无法完成内容投影的分配身份逃逸仍明确拒绝，不输出成功文档；这不表示整个 semantic-effects.run 已完成。

真实同步只读调用的 `boundary_call` 可附带 `checkpoint: {at: before, base: entry, fields: [...]}`。fields 使用与出口 S 相同的 state 值记录，描述调用瞬间已触达业务边界字段相对入口的完整差量，不表示该端口实际读取了全部字段；未逃逸局部对象不进入快照。其余字段以入口状态为基准，未知跨参数别名仍由对象身份相等的 CASE 决定。端口可能沿引用字段到达其他边界对象，因此不能只凭参数路径拼写筛掉快照字段。

出口 S 独立以入口为基准，不继承 checkpoint。例如 `value=7; inspect(state); value=9` 的调用前字段值是 7，出口字段值是 9；`old=value; value=7; inspect(state); value=old` 的调用前值仍为 7，出口显式记录恢复 E。这个恢复记录保留真实中间观察与出口值的区别，不是重新引入写入事件。两个完整、独立手推黄金位于 `tests/pers-contract/state-checkpoint/`。

本合同支持现有合同的 sync / heap_effect:none / 无 callbacks，参数与返回值沿闭合类型合同处理，包含不可变复合值和引用。声明 throws 的调用以同一响应的完成值区分正常值与各类异常载荷，S 中 completion 列出 returns 与 throws 的 exception/payload_type；正常、异常均保留已发生调用和此前字段值。引用参数须使用已有的 reference_value 观察合同；它不授权保留引用或异步读取。无引用参数的 scalar_value / immutable_value 调用保留调用及其响应身份，但没有 heap checkpoint；不能从无参调用猜测它会读取 VO。其他合法但尚未支持的合同仍明确拒绝，不改变源输入缩小合同。

入口 E 与调用响应共享按因果顺序分配的 `$e_N`。相同参数的两次真实调用仍是两个不同身份，丢弃响应也不删除调用。获取条件、参数、checkpoint 值依赖必须指向此前已定义的观察；前向或循环依赖明确拒绝。异常前已经发生的调用及快照保留，正常域 R 与异常载荷仍分别表达。

验证同时使用真实 React／reference-state 输入的内部关系断言与正式 CLI 的独立状态预期；内部行为解释差分不代替最终结构验收。

## 分支规模与入口观察域

同一后继处的兼容状态会汇合：局部绑定值用 CASE 表达，内部写入保留各自条件并合并共同前缀；Return 与 Throw 分开，同类异常仍保留条件化载荷。字段 S 的发生域独立保存，不能把未写分支改成写 null 或写初始值。不同调用历史或分配身份不强行合并，仍受显式资源预算约束。

规模回归保留 24 个独立布尔参数，分别验证不同字段更新和同一字段的最后写入关系，输入空间仍为 2^24。它检验共享后继的归约能力，不是减少输入后跑若干代表样本。

先前写入已提供字段值的路径不获取入口 E；未知别名只有未被同对象写入覆盖的部分取得入口值。重复读取在已观察域内不会扩大 E 的获取条件；最终入口观察依赖还经过迭代拓扑检查。未能消除的自引用或依赖环明确拒绝，不能输出由自身尚未取得的值决定是否取得自身的 E 记录。

## 状态异常捕获

Catch 仅匹配声明的异常类型，并为 handler 提供该次 throw 的实际载荷。进入 handler 时保留抛出点的业务字段值、已发生调用及 checkpoint，不回滚；body 与 handler 的临时绑定只在各自词法域有效。Catch 不消费方法 Return，未匹配异常与 handler 重抛均继续传播。比如写 7 后抛出载荷 9，handler 写入载荷后，出口字段是 9；如果未匹配或重抛前未改字段，出口仍为 7。该控制关系归约为现有 R/E/T/S/throws，不增加执行式 catch 节点。

## 内部整成员选择

内部选择复用已确认的 choice 合同，在 T 中以 member_choice 声明允许成员与 empty fallback；R、字段 S 和后续调用共同引用同一个关系绑定。它不占用外部 E/RPC 编号，两次实际选择也不因候选相同而合并。仅在各自发生条件成立时关系有效，未支持的循环组合仍明确拒绝。完整语义和诊断模式的区别见仓库 `docs/normalized-choice.md`。

## 观察获取域与共享递推

E、调用响应和 T 仅在各自获取条件成立时有定义。条件合并保留原分支，在 Expr 层以 CASE 保护未获取观察；不能通过无条件求全部观察或把互斥获取域相与来删掉合法路径。该保护同时用于 E/S、调用及 checkpoint 条件、T.when 和 throws，R 仍只在正常完成域有定义。

已具备实时状态合同的有序标量 fold 在 execution 期间直接更新 `T` 定义，返回边界发布实时状态中的 `R/T/throws`，不再为该切片调用 post-execution fold reducer。仍未具备完整实时状态合同的复杂状态递推、member_choice、循环异常与一般返回端口引用继续使用兼容状态归约，直到 stack/heap、completion 和观察域都有独立证明。循环前已写字段不回滚。一般返回的端口引用仍属于同一环境堆：先写 state.value=7 再读取 reply.value，需以 reply==state 的 CASE 转发 7，其他身份读取其入口 E，不能把所有返回引用当新对象。只有无参数、无字段状态、无读写、无异常的一次同步 `reference_value` 调用，且返回区与唯一 S 事件保持同一外部身份时，才可走 realtime identity-only 直出；其他返回引用继续使用兼容状态归约。
在这条 identity-only 合同上又允许一个窄分支：同一外部引用只可用于一个 `== null` 或 `!= null` guard，两个分支都必须返回不携带堆身份的标量。execution 保留引用身份，direct renderer 将 guard 中的临时 `HeapRef` 映射为对应的 `$e_n`，同时发布 guarded `R` 和唯一 `S`；引用进入字段、参数、写入、回调、异常或其他 alias 关系时仍回退兼容状态归约。
同一合同还允许一个唯一外部引用的只读字段投影。只有一个声明字段、一个同步无参数 `reference_value` 调用、一个 `reference != null` 守卫读取和标量返回同时成立时，`E` 才直接记录该字段，`R` 使用该字段的外部绑定，`S` 保留调用顺序，空路径在 `throws` 中保留受守卫的 `NullPointerException`。字段写入、多个未观测字段、字段值携带 heap identity 或其他 alias 关系继续使用兼容状态归约。
外部引用也可在同一实时合同中作为下一次同步端口的唯一 `reference_value` 参数。此时 `S` 必须保留第一次返回和第二次转发的顺序，第二次调用返回的外部 `E` 直接进入 `R`，不把中间引用复制为新的 heap identity。只允许两次无异常调用和无字段状态，重复转发、字段写入、回调、异常或其他 alias 关系继续使用兼容状态归约。
两个不同的外部引用身份还可以在一个更窄的标量比较合同中先后返回，并在 execution 期间把 `left == right` 的分支直接 join 为 `R` 的 `CASE`。合同必须只有两个无参数、同步、无异常的 `reference_value` 事件，结果分支必须是这两个 identity 的一次相等判断，两个分支值必须是无堆标量。`S` 按调用顺序保留两个 `$e_n`，`R` 直接引用同一组 `$e_n`。字段、写入、参数转发、异常、第三个 identity 或其他比较运算继续使用兼容状态归约。
同一比较合同也允许引用值结果，但两个 `CASE` 臂只能返回此前两个外部 identity 之一。`R` 因此保留 `$e_n` 的别名关系，仍不产生新的 heap identity。字段、写入、异常、额外 identity 和其他 alias 关系继续使用兼容状态归约。

值自身也受所属域约束：R 在全部正常完成域内检查，字段 S 的 target/value 在其 cond 域内检查，checkpoint 字段、调用参数、E target 和异常载荷各用自己的发生域。值内 CASE 的每个条件先保护获取域，再在对应臂域检查值；不能只证明外层 cond 合法就输出某个尚未获取的响应。不同正常返回值保留完整有域 CASE，同值正常返回可直接使用共同值。

复合获取域的反面可能仍需细分。例如先检查列表非空、再读取首元素.ext：未取得 ext 既可能因为空列表，也可能因为首元素为 null。归约只使用当时已经取得的布尔条件继续分区，分别保留越界与空引用异常；无法取得的裸值读取仍拒绝，不补 null 或 false。

同一入口快照的有限索引绑定见[有限索引的入口状态观察](final-indexed-state-pers.md)。该文档规定测试目标形式，生产状态归约尚未接通该结构。

## 独立入口快照的 E 编号

无调用、索引调用族、内部选择和 T 定义的状态结果，可以对同一直接入口对象、同一获取条件的字段观察应用稳定编号。入口对象由 Heap 身份及显示名与合同 Object 参数的对应关系认证；字段类型来自 heap_fields，条件不依赖行、E 或 T，涉及参数必须在合同中声明。这个子集按 `(field,type)` 排列实际使用的字段并统一命名 `$e_0,$e_1,…`，E 按数字编号输出，所有最终表达式同步引用新名。

该规则针对合同规定的入口状态快照；同一快照中互相独立字段的访问先后不应决定最终名字。获取域、字段值、别名和异常条件原样保留，重复字段不合并不同观察身份。它不推断原语言 getter 或 volatile 读取可以交换。

不同根、不同获取条件，或者带调用、选择、递推定义的结果仍沿用既有因果身份；不满足认证前提不强行排序。跨全图 E／T 的稳定重编号是独立能力，不能由局部快照规则或测试侧名称映射代替。完整原则见[规范形式合同](normal-form-contract.md)。
