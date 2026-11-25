# 纯循环的最终 PERS

`normalized_pers_fold` 将无 heap、端口、task、catch 的值循环归约为已有 `T.op: fold_until` 数学递推。`step.next_state` 和 `step.continue` 都是纯表达式；每步先提交 next_state，再按 continue 决定是否继续。Vector 输入使用 `vector_sequence` 与纯值表达式，例如 `value.indices(count)`，不包含执行节点。

循环内显式异常以纯完成值表达：状态类型为 `record<exception:i32,state:...>`，有可观察 payload 时增加对应的 `payloadN` 字段。零标签表示正常；非零标签由排序后的异常类分配。异常步保留最后已提交的业务状态并停止，不能把异常步骤未提交的 state 当作正常结果。最终 `R` 投影业务状态，`throws` 投影异常标签和 payload；外部输入为空等循环前异常保留原条件。异常完成值是吸收状态，不会继续消费后续输入。此规则限于无 heap 写入的值循环；有写入的异常步骤需要保存抛出前已发生的可观察更新，不能直接套用旧业务 state。

`pers_fold_completion` 从 `semantic-composite-state/Original.java` 独立构造完整文档，使用已有 math/formatter 规范化手推公式。这验证源码到归约结果的计算链，不是独立验证 math 化简器。

- `balanced`：0 停止；40/91 入栈；41/93 仅匹配栈顶时出栈；其余输入或不匹配抛出异常。空输入列表为正常真值，null 输入仍抛 NPE。异常保留旧栈。
- 动态 typed-step：在原 budget 步骤之前增加 `item == 99` 则抛 `InvalidItem(index)`。其余步骤先 push，再依据旧长度与 limit 比较停止。例如 `[1,99]`、limit=0 在第一步正常停止，不读到 99；`[1,2,99,4]`、limit=10 在 index=2 抛出，已提交向量为 `[1,2]`。

balanced 的源码公式为 `top == CASE x==41 THEN 40 ELSE 91 END`。在 `x==41` 域可由整数相等定义特化为 `top==40`；测试仍以唯一规范完整文档验收，不保留替代表达式列表。源码分类验证覆盖 x 的 41/93/其他类与 top 的 40/91/其他类，并拒绝错误栈顶、反向字符和开闭边界变化。

嵌套纯向量循环使用同一文档中的参数化 T 定义：`parameters` 声明完整值类型，`$t_n(args)` 每次按实参从 initial 求值。带参定义的 `when` 必须为 `true`，条件调用由外层 CASE 控制；尚未实现的参数化部分域不能通过认证。输入向量、捕获值以及有自由依赖的初值都显式参数化；不把内层循环提前计算一次，不隐藏外层 row、入口 P、E 或 HeapRef。实际实参类型来自现有声明和已证明的字段投影规则，不从表达式文字猜测。定义依赖只能指向先前完整定义；每个文档使用独立的内部函数身份，最终 formatter 仅按该文档已验证的精确映射打印 T 应用。

`pers_nested_contract` 从未修改的 `fixture-nested-fold/Probe.java` 固定内外层方程：内层逐项提交 `subtotal + (x + before)` 后判断 x 是否非负，外层提交 `before + subtotal` 后判断 index 是否小于 stop，全部加法保留 word64。`[[1,2],[3,4]]` 得 16，`[[1,2],[-1,100]]` 得 5；stop=0 时跳过后续 null 行，不能提前抛异常。各比较的互逆写法（x>=0 / 0<=x，index<stop / stop>index）按既有整数序规范统一；测试只接受唯一规范完整文档，独立源码分类仍拒绝方向或开闭边界错误。

入口状态读取仍由 state reducer 保留真实 E 获取域。条件中的观察依赖在 Expr 层按域形成 CASE；例如 source=null 时先得到 NPE 真值，非空分支才能读取外层 T 的异常标签。CASE 未激活臂不是读取，两个互斥获取域不能错误地相与。域外裸读、循环获取依赖和资源预算耗尽均明确报错，不能变成 false。

循环内 heap、端口和 catch 等未支持操作仍明确拒绝。不能以内部 Behavior IR 充当上述最终 PERS。

### 关系源的最终 PERS 结构

纯 fold 的 `input` 可以继续携带统一关系代数。关系源在最终 PERS 中保持为可递归检查的结构对象，不能退化成内部关系调试文本，也不能因为它不是 `Base`、`Select` 或 `Empty` 就被静默丢弃。当前结构源包括：

- `input_sequence` 和 `vector_sequence` 表示外部输入与已绑定的不可变向量。
- `singleton_sequence`、`singleton_kv_sequence` 表示单元素关系。
- `project_sequence`、`project_kv_sequence` 表示投影，`input` 保留投影前关系。
- `distinct_sequence`、`sorted_sequence` 表示去重和排序，二者都保留被处理的关系。
- `union_sequence` 保持集合并集的各输入，`ordered_concat_sequence` 按输入顺序保持拼接关系。
- `difference_sequence` 同时保持 `left` 与 `right`，避免把差集误降为只剩左侧过滤。

这些结构源可以嵌套，也可以继续作为下一层 fold 的输入。源内表达式仍须经过同一作用域检查和 formatter，关系算子本身不代表执行节点。`ActiveSet` 只有在历史被完整归约后才能进入这里；未闭合的活动关系继续 fail closed，避免把不完整的事件历史伪装成纯关系 PERS。

### Immutable vector type evidence

The internal `IterationInput::Vector.value_type` retains the complete vector type already checked by the normalized parser. It is optional for older internal producers and does not add a field to final PERS or change the historical diagnostic renderer. A consumer must establish the same complete type from binding evidence, or validate a literal array/record/CASE against that declaration. Integer literals do not independently choose i32 or i64; declared i32 elements must fit i32. Unknown symbols, conflicting declarations and unproved composite constructors fail closed. Validation runs during whole-body admission, including branches not selected by a particular output path.

`pers_decimal_format_contract` fixes the source equations for both decimal-format entries before comparing CLI output. The inner nine-element i32 vector computes one decimal digit by repeated subtraction of a negative power; the outer nineteen-element i64 vector handles the entire signed long domain, including MIN_VALUE. The nine and nineteen bounds come from base ten and the declared long width. Runtime zero digit and negative prefix remain symbolic, and the visible Money constructor's constant 123 behavior does not restrict its raw text/null input. The test is an independent source formula using the shared mathematical formatter, not an independent proof of that formatter.

## 必须应用的纯递推等式

最终 PERS 不仅需要纯表达式结构，还必须应用已定义且适用的数学归约。规则依据完整 `PureDefinition` 的输入、状态类型和递推方程，不按入口名或测试项目选择输出，也不保留多个黄金候选。

- 空不可变值向量返回初值；结构性单元素向量提交一次 `next_state`。元素仍可为未知参数，不使用运行样本替代输入。
- 纯 identity copy 返回相同值序列。该等式不适用于可观察的堆分配身份。已有 Source 上的过滤复制保留 Select 与真实行绑定；后续 Count/Exists 消费不保留多余复制递推。
- 从零开始每项递增的 w 位有符号计数归约为 `signed_word_w(Count(input))`，保留重复项、过滤谓词和原位宽。依赖旧 state/index 的谓词或未经证明的提前停止不适用此等式。
- false 初值、true 吸收态且未命中前继续的纯递推归约为 Exists；相反初值和吸收态归约为 Not Exists。
- 每项覆盖同一纯值且值不依赖 item/index/old state：空输入返回 initial，非空输入返回该值。

上述规则先通过完整定义认证，再检查新表达式的词法作用域；新引入的自由 fold row 明确失败。关系内行绑定只在该关系的查询范围有效。正常执行域不因删除 T 而变成全域，E 的获取和异常条件继续由原入口路径保持。参数化定义、异常完成记录及历史证书不因这些局部等式而绕过自身认证。

`pers_simplification_contract` 的完整源码预期覆盖 singleton、copy/filtered-copy 消费、带位宽计数和存在性、原观察域、空/null 异常。生产单测另覆盖早停、捕获值、参数化不删除、自由行拒绝及重复 normalize 稳定性。共享 math/formatter 构造的源码预期用于端到端校验；数学规则本身另有直接等式和反例测试。
