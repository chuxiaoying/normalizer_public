# Hello World：关系排序、LIMIT 和空结果默认值

## 设计意图

关系格式化也验收作用域：数组首项如果有筛选或局部排序，不能退回无条件读取下标零；纯布尔守卫和已取得的外部比较阈值保留各自来源，不被重命名成物理列。

检验过滤、排序、截断和空结果缺省的先后关系。空集与有命中但值为零不同；移动排序或改方向会改变首项，不能按输出外观合并。行投影可包含局部参数或计算值；只有真实读取的物理列进入 E，未用的投影不新增表列，也不能替换筛选引用的物理列。交换等值筛选的两侧不改变取得范围。查询内的逐行绑定用于表达式，不是新列或 P；将选中行的到期日加一，不能改变排序先后或空集缺省。即使某个文本列只在 WHERE 中读取而不投影，它仍是 E 中依赖的物理列；筛选须早于排序和取首项。用单键排序会选择不同的首行但不改变投影；来自后续关系步骤的额外数值界限也必须先于 LIMIT。

## 输入原始代码

从账单关系中按到期日和主键选择最早一行。来源表的物理约束是 `bill.id` 为非空唯一主键；这不是从列名、`int` 类型或一次样例数据推断出来的。三个字段都是非空业务整数；没有匹配行时返回零。

```java
// 数据源约束：bill.id 是非空唯一主键。
class BillOrderRow {
    int accountId;
    int dueDate;
    int id;
}
class DueDateService {
    static int firstDueDate(int accountId) {
        BillOrderRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { accountId: b.account_id, dueDate: b.due_date, id: b.id }
        */
        int dueDate;
        /* @normalizer-ir
           dueDate = first from bills as row
               order by row.dueDate, row.id
               yield row.dueDate else 0
        */
        return dueDate;
    }
}
```

## AI 预处理结果

**原样通过。** 局部关系 IR 已经保留排序键、首项选择和空结果默认值。AI 不把排序改成字符串比较，也不把首项改成无序任意选择。

**预处理要点**：关系首项的稳定顺序是业务语义，必须由 `ORDER BY` 和 `LIMIT 1` 一起表达，不能只保留“取一行”。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/DueDateService.java#DueDateService.firstDueDate
  returns: number

P:
  parameters:
    accountId: { position: 0, type: number }

E:
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      due_date: number
      id: number

R: |-
  COALESCE(
    FIRST(bill.due_date
      FROM bill
      WHERE bill.account_id == accountId
      ORDER BY bill.due_date, bill.id
    ),
    0
  )
```

**PERS 验收要点**：排序和 LIMIT 共同决定取得哪一行，空集默认值属于正常业务结果，不能被省略或改成异常。

输入账户 7 有两张账单 `(id=1, due_date=20)`、`(id=2, due_date=10)`，另一账户有一张到期日为 1 的账单。按到期日优先返回 10，按主键优先返回 20，查询不存在的账户返回 0。两行到期日相同时，唯一的主键仍决定先后；这个标量只返回到期日，无法从值区分两行，因此测试另外逐项核对 PERS 中的 ORDER BY。只按主键排序的变体仍有全序，只改变排序，不改变投影。如果更换数据源而没有主键唯一性的证据，则不能把会返回不同值的并列行描述成稳定首行。

查询局部名只在本条查询里绑定。预处理新引入计算名时选不冲突的名称，不改变账户输入或行别名的原绑定；若交付后名称无法唯一解析，按 [AI-11](../../../../design/chapters/04-AI救赎.md#ai-11-分析副本先通过静态校验) 报实际的绑定问题。行内计算只进入选中行的投影。
