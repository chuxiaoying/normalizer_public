# Hello World：分组、聚合和空组

## 设计意图

分组投影和普通去重或并集是不同的关系结构。格式化只在识别出对应的已声明形状时使用专用 GROUP BY／UNION 表达，不根据一个相同字段名猜测整个查询的类别。

检验分组键、聚合值、空组和过滤的位置。先过滤还是先分组可能改变记录与计数，不能将组数、行数或业务值加总互换。

## 输入原始代码

统计账单关系中不同状态的数量。没有账单时返回零。

```java
class BillGroupRow {
    String status;
    int account_id;
}
class GroupService {
    static int statusGroupCount() {
        BillGroupRow[] bills;
        /* @normalizer-ir
           bills = from bill as row yield { status: row.status, account_id: row.account_id }
        */
        int count;
        /* @normalizer-ir
           count = count groups from bills as b by b.status
        */
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** 工程声明的行类以数组承载查询结果，不在分析副本依赖 JDK 集合类。 局部关系 IR 明确交付分组键和组数聚合。AI 不把分组结果展开成固定状态列表。

**预处理要点**：分组键、组内聚合和空关系结果必须分别进入关系表达，不能从当前样例碰巧出现的状态值反推出固定分支。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/GroupService.java#GroupService.statusGroupCount
  returns: number

P:
  parameters: {}

E:
  bill:
    kind: table
    source: bill
    columns:
      status: text
      account_id: number

R: |-
  COALESCE(
    SUM(
      SELECT 1
      FROM (
        SELECT bill.status
        FROM bill
        GROUP BY bill.status
      ) AS groups
    ),
    0
  )
```

分组键不是表声明。切换键不改变 `bill` 的字段合同。例如有 `(OPEN,1), (OPEN,1), (OPEN,2), (CLOSED,1)` 四条记录，按状态、账号、两项联合分组的组数分别为 2、2、3。空表为 0。

**PERS 验收要点**：GROUP BY 的结果是关系集合，组数取决于实际输入，不能把当前输入中的状态值预先列成常量 CASE。

行投影可把 `bill.status` 绑定为局部字段 `category`。按这个投影键分组仍按原表的 `bill.status` 分组；物理表及列合同不变，局部字段名不作为另一张表或另一个物理列发布。
