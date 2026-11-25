# Hello World：continue 与提前 return 的关系选择

## 输入原始代码

这个案例展示循环中的 `continue` 和提前 `return`。AI 预处理保留过滤条件、顺序和首个匹配语义，转换为无循环的 `first` 关系选择。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class ArrearsService {
    private final BillMapper billMapper;

    ArrearsService(BillMapper billMapper) {
        this.billMapper = billMapper;
    }

    int firstPayableAmount(int accountId, int today) {
        for (BillRow row : billMapper.selectOpenBills(accountId)) {
            if (row.dueDate > today) {
                continue;
            }
            if (row.amount > 0) {
                return row.amount;
            }
        }
        return 0;
    }
}

interface BillMapper {
    @Select("""
        SELECT id,
               account_id AS accountId,
               due_date AS dueDate,
               amount,
               priority
        FROM bill
        WHERE account_id = #{accountId}
          AND status = 'OPEN'
        ORDER BY priority, due_date, id
    """)
    List<BillRow> selectOpenBills(int accountId);
}

class BillRow {
    int id;
    int accountId;
    int dueDate;
    int amount;
    int priority;
}
```

Mapper 先按优先级、到期日和 ID 排序，Java 遍历沿用查询返回的行序。三个排序键完全相等时，原 SQL 没有进一步规定顺序，本例不额外假设 ID 唯一。

## AI 预处理结果

查询位置的注释按原 SQL 筛选、排序并生成 `rows`。循环位置的 `first` 从这组有序行中选择首个已到期且金额为正的账单。

```diff
--- a/src/ArrearsService.java
+++ b/src/ArrearsService.java
@@ -1,39 +1,28 @@
 import java.util.List;
-import org.apache.ibatis.annotations.Select;

 class ArrearsService {
-    private final BillMapper billMapper;
+    int firstPayableAmount(int accountId, int today) {
+        List<BillRow> rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId and b.status == "OPEN"
+               order by b.priority, b.due_date, b.id
+               yield { id: b.id, accountId: b.account_id, dueDate: b.due_date,
+                       amount: b.amount, priority: b.priority }
+        */

-    ArrearsService(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
-    int firstPayableAmount(int accountId, int today) {
-        for (BillRow row : billMapper.selectOpenBills(accountId)) {
-            if (row.dueDate > today) {
-                continue;
-            }
-            if (row.amount > 0) {
-                return row.amount;
-            }
+        BillRow match;
+        /* @normalizer-ir
+           match = first from rows as row
+               where row.dueDate <= today and row.amount > 0
+               yield row
+               else null
+        */
+        if (match != null) {
+            return match.amount;
         }
         return 0;
     }
-}
-
-interface BillMapper {
-    @Select("""
-        SELECT id,
-               account_id AS accountId,
-               due_date AS dueDate,
-               amount,
-               priority
-        FROM bill
-        WHERE account_id = #{accountId}
-          AND status = 'OPEN'
-        ORDER BY priority, due_date, id
-    """)
-    List<BillRow> selectOpenBills(int accountId);
 }

 class BillRow {
```

命中时 Java 仍提前返回，未命中时走方法末尾的 `return 0;`。内核沿 `rows` 取得查询的行序，推导最终筛选、排序和首行选择。

**预处理要点**：有序选择应作为完整语义交付。需要保留的是决定首项的排序规则；不应要求 AI 将它改造成手工比较每个键、维护最优行的另一套算法。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/ArrearsService.java#ArrearsService.firstPayableAmount
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number
    today:
      position: 1
      type: number

E:
  bill:
    kind: table
    source: bill
    columns:
      id: number
      account_id: number
      due_date: number
      amount: number
      priority: number
      status: text

R: |-
  COALESCE(
    (
      SELECT bill.amount
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.due_date <= today
        AND bill.amount > 0
        AND bill.status == "OPEN"
      ORDER BY bill.priority, bill.due_date, bill.id
      LIMIT 1
    ),
    0
  )
```

最终 `ORDER BY` 沿用 Mapper 的三个排序键，`LIMIT 1` 对应循环的首次命中，`COALESCE` 对应方法末尾的默认返回值。

**PERS 验收要点**：归一化应显露业务选择顺序。循环过程可以消失，改变选中账单的排序依赖必须留下，也不能补入源码未规定的并列顺序。
