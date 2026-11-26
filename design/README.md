# Normalizer 设计

在 macOS 的 Finder 双击 `design/open.command`，或从仓库根目录运行：

```sh
./design/open.command
```

脚本会自动打开本地阅览器。默认端口是 4176；如果被其他程序占用，会换一个可用端口，实际地址显示在终端。再次启动会关闭同一份仓库中已确认的旧阅览器。首次启动需要 Node.js 24 或更高版本和 npm，缺少的项目依赖会自动安装，不需要构建 Rust 内核。保留启动窗口即可继续浏览，按 Ctrl+C 停止。更多说明见[阅览器文档](../implementation/apps/design-reader/README.md)。

## 设计思想

1. [JVM 模拟器：执行即归约](chapters/01-jvm-symbolic-execution.md)
2. [多重宇宙的分裂坍缩](chapters/02-universe-split-collapse.md)
3. [数学化简：把业务条件算出来](chapters/03-mathematical-simplification.md)
4. [AI 救赎](chapters/04-ai-boundary.md)
5. [关系代数：让 SQL 和集合运算一起化简](chapters/05-relational-algebra.md)

设计种子和派生案例的覆盖原则见[种子覆盖审查](seed-coverage-audit.md)。

先选一个你熟悉的例子，按原始代码、AI 预处理结果、理想归一化结果的顺序读。比如一个函数只做加一，AI 可以原样交付源码，最终 PERS 的返回表达式就是 `value + 1`。若对结果有不同理解，可以指出一组输入，看看两种写法在哪一步产生不同的业务结果。

[架构指导](architecture.md)用案例说明 AI、多语言解析器和符号执行内核如何分工，以及一次执行遇到未知条件时如何分裂、汇合和化简。

[统一正则规范](unified-canonical-form.md)规定哪些等价表达式必须归到同一种形态，以及排序、CASE WHEN 和观察边界如何验收。

[设计种子目录](examples/README.md)按基础主题和联合主题组织案例。[种子分母与覆盖模型](examples/coverage.md)说明应该覆盖哪些语义场景。[种子清单](examples/seeds.yaml)给每篇案例一个稳定 ID，[种子语义清单](examples/seed-manifest.yaml)记录主主题和关联主题；[测试区](../tests/README.md)按这个 ID 关联入口与变体。移动案例时更新清单里的路径，保留 ID。

## 从一段业务代码读到 PERS

| 案例 | 看哪一个区别 |
|---|---|
| [整数加一](examples/topics/expression-simplification/java-static.md) | 输入取 Java 最大整数，业务上的“加一”应该得到多大？ |
| [三条返回路径](examples/topics/control-flow/java-static-case-when.md) | 调整值恰好等于上限时，为什么一定要返回备用值？ |
| [多层纯函数调用](examples/topics/expression-simplification/java-static-deep-calls.md) | 加一、加基数、再封顶，能否从最终公式直接读出上限？ |
| [返回一个对象](examples/topics/objects-and-types/java-static-object.md) | 当选中的值等于抬高后的值，返回对象里的差额应是多少？ |
| [算术、方程与布尔化简](examples/topics/expression-simplification/java-expression-simplification.md) | 为什么只有 `x = 6`、`y = 4` 时返回基础值？切换 `preferred` 会怎样？ |
| [参数与状态同名](examples/topics/basic-contracts/java-name-collision.md) | 新旧额度都叫 `quota`，算差额时如何分清取的是哪一个？ |
| [外部文本库](examples/topics/external-boundaries/java-external-library.md) | 输入全为空白时返回什么？怎样在原调用处交代这条规则？ |
| [查询、插入、更新账单](examples/topics/relational-data/java-static-external-sql.md) | 同是写数据库，为什么插入一行和更新一批账单会给出不同的行数公式？ |
| [循环累加](examples/topics/loops-and-termination/java-loop-fold.md) | 只合计本账户未关闭账单的正金额，没有符合的账单时应返回多少？ |
| [循环提前返回](examples/topics/loops-and-termination/java-loop-early-exit.md) | 两笔账单都符合条件时，哪一笔的金额胜出？ |
| [固定重试与积分循环](examples/topics/loops-and-termination/java-loop-bounded-retry.md) | 最多尝试三次，第一次成功返回零时还该继续重试吗？ |
| [无上界的外部轮询](examples/topics/loops-and-termination/java-loop-unknown.md) | 无法得知服务何时就绪，为什么此例应停在预处理阶段？ |
| [Vue 生成欠费页数据](examples/topics/language-entries/frontend-vo-production-vue.md) | 两次请求都成功和第二次失败时，页面上会留下哪些不同状态？ |
| [Vue 从欠费页数据渲染卡片](examples/topics/language-entries/frontend-vo-consumption-vue.md) | 同一个页面状态，怎样分别决定卡片金额和“加载中”文案？ |

## 容易看错的业务边界

按主题阅读边界案例：[同一对象与不同对象的写后读取](examples/topics/state-and-aliasing/java-state-alias.md)、[显式空值与缺失键](examples/topics/collections/java-map-present-null.md)、[按整行去重与按账户去重](examples/topics/relational-data/sql-projected-distinct.md)。
