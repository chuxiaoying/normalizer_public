# Normalizer 设计

先选一个你熟悉的例子，按原始代码、AI 预处理结果、理想归一化结果的顺序读。比如一个函数只做加一，AI 可以原样交付源码，最终 PERS 的返回表达式就是 `value + 1`。若对结果有不同理解，可以指出一组输入，看看两种写法在哪一步产生不同的业务结果。

[架构指导](architecture.md)用案例说明 AI、多语言解析器和符号执行内核如何分工，以及一次执行遇到未知条件时如何分裂、汇合和化简。

[设计种子目录](examples/README.md)按基础主题和联合主题组织案例。[种子分母与覆盖模型](examples/coverage.md)说明应该覆盖哪些语义场景。[种子清单](examples/seeds.yaml)给每篇案例一个稳定 ID，[种子语义清单](examples/seed-manifest.yaml)记录主主题和关联主题；[测试区](../tests/README.md)按这个 ID 关联入口与变体。移动案例时更新清单里的路径，保留 ID。

## 从一段业务代码读到 PERS

| 案例 | 看哪一个区别 |
|---|---|
| [整数加一](examples/series-1/java-static.md) | 输入取 Java 最大整数，业务上的“加一”应该得到多大？ |
| [三条返回路径](examples/series-1/java-static-case-when.md) | 调整值恰好等于上限时，为什么一定要返回备用值？ |
| [多层纯函数调用](examples/series-1/java-static-deep-calls.md) | 加一、加基数、再封顶，能否从最终公式直接读出上限？ |
| [返回一个对象](examples/series-1/java-static-object.md) | 当选中的值等于抬高后的值，返回对象里的差额应是多少？ |
| [算术、方程与布尔化简](examples/series-1/java-expression-simplification.md) | 为什么只有 `x = 6`、`y = 4` 时返回基础值？切换 `preferred` 会怎样？ |
| [参数与状态同名](examples/series-1/java-name-collision.md) | 新旧额度都叫 `quota`，算差额时如何分清取的是哪一个？ |
| [外部文本库](examples/series-1/java-external-library.md) | 输入全为空白时返回什么？怎样在原调用处交代这条规则？ |
| [查询、插入、更新账单](examples/series-1/java-static-external-sql.md) | 同是写数据库，为什么插入一行和更新一批账单会给出不同的行数公式？ |
| [循环累加](examples/series-1/java-loop-fold.md) | 只合计本账户未关闭账单的正金额，没有符合的账单时应返回多少？ |
| [循环提前返回](examples/series-1/java-loop-early-exit.md) | 两笔账单都符合条件时，哪一笔的金额胜出？ |
| [固定重试与积分循环](examples/series-1/java-loop-bounded-retry.md) | 最多尝试三次，第一次成功返回零时还该继续重试吗？ |
| [无上界的外部轮询](examples/series-1/java-loop-unknown.md) | 无法得知服务何时就绪，为什么此例应停在预处理阶段？ |
| [Vue 生成欠费页数据](examples/series-1/frontend-vo-production-vue.md) | 两次请求都成功和第二次失败时，页面上会留下哪些不同状态？ |
| [Vue 从欠费页数据渲染卡片](examples/series-1/frontend-vo-consumption-vue.md) | 同一个页面状态，怎样分别决定卡片金额和“加载中”文案？ |

## 容易看错的业务边界

[十个对照案例](examples/series-2/README.md)从具体输入辨别结果。例如两个对象的初始额度相同，改写一个后，另一个应读到多少？把账单按整行去重，和只按账户去重，在哪两条数据上会数出不同答案？
