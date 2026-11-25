# SQL 关系场景的 PERS 形态设计

本文定义 SQL 关系场景在最终 PERS 中的 YAML 形态。SQL 只读查询、Java 集合后加工和最终业务返回汇聚为同一个关系表达式，表和列在 E 中按嵌套关系声明，R 和 S 只引用已声明名称。

## 设计目标

最终结果需要保留：

```text
SQL 表关系
→ SQL 原始谓词
→ Java 后加工谓词
→ Java 投影
→ Java 集合过滤或聚合
→ 外层业务 CASE
→ R
```

只读 SQL 表属于 E。它不会因为来自 Mapper 就自动生成 SQL S。S 记录 external、写操作、事务、锁和其他可观察边界。

## 最小来源定义

P 和 E 每个名称都必须有明确来源。公共来源放父级，局部来源放子级：

```yaml
entry:
  function: src/ArrearsService.java#ArrearsService.calculatePayable
  signature:
    parameters: [number, date]
    returns: money

P:
  parameters:
    accountId:
      position: 0
      type: number
    today:
      position: 1
      type: date

E:
  bill:
    account_id: number
    due_date: date
    amount: money
    waived_amount: money
    priority: number
    status: text
```

公式中使用 `bill`、`bill.account_id` 和 `bill.amount`。`bill` 本身就是数据库表的命名空间，列直接嵌套在其下。SQL 别名在 AI 预处理阶段消化，PERS 只保留物理列名和业务类型，不再重复声明一套 PERS 列名。输入提供 catalog 或 schema 时，表的完整数据库限定名作为关系来源身份保留。相同物理表的多次引用使用不同的 PERS 关系根或表达式别名区分。

## 关系表达式

YAML 负责承载关系公式。公式本身保持 SQL/关系表达式形式，下游使用通用 YAML 解析器取出公式字符串，再交给既有表达式能力：

```yaml
R: |-
  SELECT bill.account_id, bill.amount
  FROM bill
  WHERE bill.status == "OPEN"
```

关系表达式支持投影、过滤、连接、分组、聚合、排序、去重、并集、交集和差集。关系根和列必须已经在 E 中显式定义，公式中使用 `bill`、`bill.account_id` 等稳定名称。

## 数据库写入语句与关系公式分开

`S.sql` 是要交给数据库边界的写入语句，使用 SQL 的比较符 `=` 和单引号文本，例如 `WHERE bill.status = 'CLOSED'`；常量里的单引号按 SQL 转义。参数仍是已绑定的 P/E 引用，不得拼接未验证的字符串或动态标识符。以 `R` 中同名字段表达的关系公式则继续使用 `status == "CLOSED"`。局部 `@normalizer-ir` 的 `where` 也仍是带类型的公式，降为 `S.sql` 时才渲染成 SQL。这个区分沿用已有 UPDATE 写入合同，不改变选择行、写入顺序、返回行数或异常回执。

## SQL 与 Java 的融合

原始 SQL 提供基础关系和基础谓词：

```sql
SELECT account_id, due_date, amount, waived_amount, priority
FROM bill
WHERE account_id = #{accountId}
  AND status = 'OPEN'
```

Java 在 SQL 结果上继续做日期和优先级筛选、净额投影、正数过滤、求和与额度封顶：

```java
int total = openBills.stream()
    .filter(row -> row.dueDate <= today)
    .filter(row -> row.priority >= profile.minPriority)
    .map(row -> row.amount - row.waivedAmount)
    .filter(netAmount -> netAmount > 0)
    .mapToInt(netAmount -> netAmount)
    .sum();

return total > profile.quota ? profile.quota : total;
```

融合后的 PERS 只保留一个 R。以下聚合本身不含 CASE WHEN，即使在比较和 ELSE 分支中各出现一次，也直接内联，不提取 LET 或升级为公共 P/E：

```yaml
S:
  - call:
      external: profileClient.fetchProfile
      args:
        - accountId
      result: $e_0

R: |-
  CASE
    WHEN COALESCE(
      SUM(
        SELECT bill.amount - bill.waived_amount
        FROM bill
        WHERE bill.account_id == accountId
          AND bill.status == "OPEN"
          AND bill.due_date <= today
          AND bill.priority >= $e_0.minPriority
          AND bill.amount - bill.waived_amount > 0
      ),
      0
    ) > $e_0.quota
    THEN $e_0.quota
    ELSE COALESCE(
      SUM(
        SELECT bill.amount - bill.waived_amount
        FROM bill
        WHERE bill.account_id == accountId
          AND bill.status == "OPEN"
          AND bill.due_date <= today
          AND bill.priority >= $e_0.minPriority
          AND bill.amount - bill.waived_amount > 0
      ),
      0
    )
  END
```

这里的融合关系是 `account_id` 和 `status` 的 SQL 谓词、日期和优先级谓词、净额投影、正数过滤、求和与额度 CASE。Java 空流求和返回 0，因此 PERS 保留 `coalesce`。SQL 聚合空结果和 Java 空流结果只有在业务合同证明一致时才能合并。

## 连接、分组和排序

连接、分组和排序保持关系表达式形式：

```yaml
R: |-
  SELECT bill.amount, policy.quota
  FROM bill
  JOIN policy ON policy.account_id == bill.account_id
```

分组和 HAVING 保留分组边界：

```yaml
R: |-
  SELECT bill.account_id, SUM(bill.amount)
  FROM bill
  GROUP BY bill.account_id
  HAVING SUM(bill.amount) > 0
```

排序是可观察语义：

```yaml
R: |-
  FIRST(bill.amount FROM bill ORDER BY bill.due_date ASC, bill.amount DESC)
```

只有证明结果不依赖顺序时，才能删除排序或把排序与纯集合操作合并。

## 集合运算

集合运算保留重复和顺序语义：

```yaml
R: |-
  SELECT current_bill.amount FROM current_bill
  UNION ALL
  SELECT archived_bill.amount FROM archived_bill
```

`union`、`union_all`、`intersect` 和 `except` 必须保持各自的集合性质。不能因当前样例输出相同而删除 `all`、去重或交换操作顺序。

## 纯化简边界

纯过滤、纯投影、纯聚合可以合并，条件包括：

1. 谓词只读取当前行、P、E、已绑定的完成值或纯常量。
2. 投影不产生异常、边界调用、状态写入或对象身份观察。
3. 空值、重复、顺序、三值逻辑和空集合结果保持原语义。
4. 合并不跨越 S、异常边界、状态写入或有序调用。

例如，下面的关系表达式可以融合：

```yaml
R: |-
  SELECT bill.amount - bill.waived_amount AS netAmount
  FROM bill
  WHERE bill.amount - bill.waived_amount > 0
```

如果投影可能抛异常，或者投影改变可观察对象身份，PERS 保留原有关系层次。

## NULL、异常和边界

关系谓词保留 SQL 三值逻辑。需要区分 NULL 时使用表达式：

```yaml
R: bill.amount IS NULL
```

关系为空、聚合结果为空、列值为 NULL 和 external 返回 NULL 是不同情况。PERS 不用一个默认值覆盖它们。

以下情况需要进入 S 或 throws：SQL 写入、事务提交、锁或触发器，数据库函数和存储过程，external 调用的发生时机、异常、重试或顺序，以及不能安全表示为关系或纯值的 Mapper 调用。

只知道 Mapper 方法名而没有 SQL 和关系合同，不能生成 E 表。AI 预处理应补齐真实关系来源，或保留为 external 边界。

## 拒绝边界

结果应拒绝以下形态：

- 未绑定的表、列、行别名、参数或 external 结果。
- 动态拼接表名、列名且没有闭合关系合同。
- 把只读 SQL 重复记录为 SQL S。
- 把 SQL 文本包装成黑盒 `sql_exec`。
- 把 Java 自有 `map`、`filter` 方法名直接当成关系操作。
- 把可能抛异常或产生副作用的后处理强行融合成纯关系算子。
- 用实现级临时名字替代 P、E 或明确的 `$e_n`。

本文只规定最终 PERS 的 YAML 结果合同。场景 parser、统一 IR 适配和内核 lowering 都应以此结果合同为目标。
