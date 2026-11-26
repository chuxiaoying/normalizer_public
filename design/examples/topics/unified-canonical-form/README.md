# 统一正则规范专题

[统一正则规范](../../../unified-canonical-form.md)是设计源头。此主题只测试「不同写法是否得到同一个唯一结构」和「不该合并的语义是否保持不同」，不重复其他主题对业务运算本身的验收。一个规则可以复用其他主题已有例子；只有例子的实际完整断言验证了这条规则，才计入覆盖。

| 规则族 | 必须有的成对例子 | 对照反例或限制 | 种子与正式验收 |
|---|---|---|---|
| 精确整数 Add／Mul | 同一组符号的排列、不同括号、重复项与常量；固定左折叠，重复归约不变 | 文本拼接、浮点、word 运算、未取得值 | [加法种子](java-associative-normal-form.md)已验收 Java Add／Mul 的排列、括号与重复因子；关联 [JavaScript 模块种子](../language-entries/javascript-module.md)验证相同 Add 规则；[文本顺序反例](java-text-order.md)证明不能按运算符外观排序。浮点、word 和完整再次规范化仍待逐项核对 |
| 总 Boolean AND／OR 与有序比较 | 交换两端且同步翻转符号；布尔排列和不同括号汇合 | 短路、可空和 NaN 不推出额外互补 | [总布尔排序种子](java-total-boolean-order.md)验证 AND／OR 各自的排列、括号和业务差异；[短路种子](../control-flow/java-short-circuit.md)验证保护边界；比较换边仍需扩充独立对照 |
| 完整二分 CASE | 正反条件与交换两臂得到同一代表；数值指示器、普通计数、min／max、Vector 各按其域 | 只取反不换臂、互换嵌套 CASE、部分 CASE 不补 ELSE、nullable 保留惰性 | [二分 CASE 种子](java-binary-case.md)已验收计数的两种互补写法、比较换边和阈值反例；[取小取大种子](java-number-selection.md)验收严格／非严格、同时翻转和不同目标的对照；同一种子已验收可空标签 0／1 指示器与非二值反例；[只读 Vector 长度](../language-entries/javascript-module.md)已验收空域代表、阈值与可空／普通数值反例；响应 Vector 的保护域仍待验收 |
| 多分支 CASE、区间和 LET | 相邻同值合并、非相邻同值按有效域汇聚、区间交并补、合法提取和归约不动点 | 有重叠 guard 的首命中不能重排；禁止无 CASE 或仅出现一次的 LET | [整数分区种子](java-integer-partition.md)已验收相邻有效域合并、开闭端点和可观察优先级；[布尔首命中](java-total-boolean-order.md)已验收不相邻同值按真实有效域合并；[局部绑定](../expression-simplification/java-case-let-extraction.md)已验收重复的完整 CASE 正例及一次、无 CASE、受保护反例；再归约仍待补 |
| 纯总不可变构造与序列观察 | 总 CASE 外提的唯一方向、零索引统一为 FIRST | 不跨惰性／部分域；保留首项 null、顺序和越界异常 | [直接读第零项](../collections/java-empty-singleton.md#直接读第零项不吞掉越界)已验收纯标量数组的 FIRST、越界和非零索引对照；[按序投影首项](../collections/java-find-first.md)已验收先判存在、保持投影及顺序、再运算；[首项为空与没有记录](../relational-data/java-null-semantics.md)已验收独立存在性和显式缺省；[不可变数值 Vector 构造](../language-entries/javascript-module.md)已验收 CASE 外提、元素位置及预算；[有序只读 Map](../language-entries/javascript-module.md#有序只读-map可空输入只在非空时插入)独立验收可空字段插入、外层 CASE、键顺序与失败边界；其他构造仍待验收；[Java 常量数组](java-constant-array.md)单独验收长度分配、位置不交换和异常，不视为不可变构造 |
| Heap、E／T 业务名字与稳定编号 | 两次分配顺序不同但公开路径相同；纯定义别名和依赖图编号稳定 | 同名不合并不同对象或调用；禁止 S 乱序、移动取得域或重编号后漏改引用 | [对象引用之对照](../objects-and-types/java-structural-vs-identity.md)已覆盖直接参数公开名优先于分配顺序，包括多个身份判断的合取；[已取得的命名状态](../state-and-aliasing/java-explicit-marker.md)已验收 E 路径优先与减法对照；纯递推 T 已覆盖独立及有依赖编号；已验收 E 状态字段作为纯递推初值；[匿名回执](../external-boundaries/java-external-reused-result.md)已验收 11 次真实调用的数字编号排序；[直接整数成员选择](java-member-choice.md)验收独立 T 及每次身份；[整记录报价选择](../collections/java-independent-choice.md)验证同一 T 的字段关联和独立第二次选择；更一般的条件组合仍待验收 |
| 顶层异常和边界结构 | 已证明互斥的异常记录独立排序；同内容完整结构严格相等 | 不交换具因果关系的异常或 S，不能丢载荷、cond 或取得条件 | 关联[批量写入](../relational-data/java-batch-write.md)及其派生用例已覆盖按异常名排列但保持 SQL 因果与 S 顺序；[直接 Java 异常载荷](java-exception-payload.md)验收消息排序、同类不同载荷、显式 null、首匹配与同值汇聚；[外部端口载荷](java-exception-payload.md#外部异常载荷必须来自它自己的回执)验证同类两次调用不合并、载荷保留原 S 回执和声明类型，未执行时无载荷 |

每一行验收都须列出实际种子 ID 与正式 CLI 入口，至少用同一份独立规范预期分别验证两种输入，并保留反例。不能用「比较两份 actual 彼此相同」代替黄金；有限域语义证明和幂等性检查是额外证据。待补不等于已通过。

[具体整数商和余数](java-constant-quotient.md) 的种子 ID 是 `canonical-constant-quotient`，经正式 Java 源码入口验收。局部变量和直接字面量必须得到同一结果；负数、越过宿主字长、零除数受保护和捕获对照各有独立预期。

### 纯递推 T 的验收边界

[纯递推定义种子](java-pure-fold-definitions.md)覆盖独立递推按公开输入排、依赖先于字典序、源私有名与交换律改写不改变发布结果。依赖外部调用的一般 E/T 图、有条件递推和 `member_choice` 不计入这一纯递推切片。
