# Normalizer 扩展语义参考

扩展写进 Java 注解或 TypeScript 的声明注释；关系指令写在语句位置的块注释里。源码的普通控制流决定何时走到这一步，扩展只补普通语言表达不出来或需要统一的含义。一个操作不能原调用和替换指令各执行一次。这是分析工程的合同，不是拿去替换生产工程的运行时。

| 层次 | Java 或 TypeScript 写法 | 作用 |
| --- | --- | --- |
| 标准值操作 | Java `import normalizer.intrinsics.N;`；TS `import * as N from "@normalizer/stdlib";`；调用如 `N.trim(x)`、`N.round(x, 2, N.HALF_UP)`、`N.plusDays(day, days)` | 普通、带类型的函数调用。按官方绑定和签名识别，不按名字猜，不在注释里另写一套纯值调用。 |
| 标准环境读取 | `N.currentTimeMillis()` | 执行到该调用才读取，不按纯函数合并。 |
| 可空性 | Java `@N.Nullable String value`；TS `string \| null` | 声明已有的值域，不负责转换或补默认值。 |
| 已有状态 | Java 字段 `@N.EntryState`；TS 声明前 `/** @N entryState */` | 绑定入口之前已存在的状态；仍从普通字段或变量读写。 |
| 组件属性入口 | TS `/** @N componentProp Component#instance.prop */` | 将导出入口绑回组件实例及属性；不会附带一次渲染。 |
| 外部端口 | Java 接收者字段 `@N.External`；TS 声明前 `/** @N external */` | 在普通签名中保留参数、结果和已知失败；标记本身不触发调用。 |
| 局部关系指令 | `/* @normalizer-ir … */` | 在原语句的位置执行。名称绑定到有类型的变量、行字段或声明过的表；不是随意放一段自然语言。 |

## 关系指令按操作组合

本页给出统一的设计写法，Java 和 TypeScript 共用一套关系语义。[《AI 救赎》](chapters/04-ai-boundary.md)说明 `N` 标准库和声明标记；[《关系代数》](chapters/05-relational-algebra.md)说明关系运算和化简。第五章的语法表还包含较窄的示例写法（如 `value.map_get_or`），用于理解对应的运算，不把它当作另一套标准。下面列的是规范的代表形状，使用时仍须满足各操作的类型、作用域和完成合同。

| 语法族 | 代表形状 |
| --- | --- |
| 查询和投影 | `from source as r where predicate yield { amount: r.amount }`；可用行内 `let`，有顺序来源才绑定索引 |
| 条件与排序 | 比较、`and`、`or`、`not`、`is [not] null`；`order by` 明确键、方向和需要的空值顺序 |
| 首项、取段、任选 | `first from values[k:] else fallback`；`choose from members else fallback` 表达任选，不冒充有序首项 |
| 连接、分组、聚合 | `join ... on ...`、`left join`；`group by`、`having`；`count from`、`sum from ... yield ...` |
| 多重集与去重 | `union all (left), (right)`；`distinct yield expr`，再按需要聚合 |
| 查键和更新 | `m = map empty`、`put m[key] = value`、`x = lookup m[key] else fallback`；只有缺键才走回退 |
| 有序折叠与整数区间 | `x = reduce values from initial as (acc, item) yield expr`、`n = count from [lo, hi) as i`；保留初值和迭代依赖 |
| 结果绑定 | `target = operation`；有对应加法合同时可用 `target += operation` |
| 表写入和完成 | `insert into`、`update ... set ... where ...`、`delete from ... where ...`；例：`returning affected_rows atomic throws ErrorType` |

声明、关系指令和原生表达式各有位置。PERS、`CASE WHEN` 和符号回执是分析结果，不是给 AI 回填到输入的语法。找不到正式表达的保证，报告缺口，不临时创造标记。
