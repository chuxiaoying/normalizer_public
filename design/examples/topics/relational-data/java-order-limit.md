# Hello World：关系排序、LIMIT 和空结果默认值

## 设计意图

检验过滤、排序、截断和空结果缺省的先后关系。空集与有命中但值为零不同；移动排序或改方向会改变首项，不能按输出外观合并。行投影可包含局部参数或计算值；只有真实读取的物理列进入 E，未用的投影不新增表列，也不能替换筛选引用的物理列。交换等值筛选的两侧不改变取得范围。

## 输入原始代码

从账单关系中按到期日和主键选择最早一行。行声明明确三个字段都是非空业务整数；没有匹配行时返回零。

```java
import java.util.List;

class BillOrderRow {
    int accountId;
    int dueDate;
    int id;
}
class DueDateService {
    static int firstDueDate(int accountId) {
        List<BillOrderRow> bills;
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
    (
      SELECT bill.due_date
      FROM bill
      WHERE bill.account_id == accountId
      ORDER BY bill.due_date, bill.id
      LIMIT 1
    ),
    0
  )
```

**PERS 验收要点**：排序和 LIMIT 共同决定取得哪一行，空集默认值属于正常业务结果，不能被省略或改成异常。

输入账户 7 有两张账单 `(id=1, due_date=20)`、`(id=2, due_date=10)`，另一账户有一张到期日为 1 的账单。按到期日优先返回 10，按主键优先返回 20，查询不存在的账户返回 0。两行到期日相同的时候，这个标量返回无法区分主键先后；测试另外逐项核对 PERS 中的 ORDER BY。
