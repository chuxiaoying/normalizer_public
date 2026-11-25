# Hello World：有序集合中的首个匹配

## 输入原始代码

入口从账单结果中寻找金额为正的首行，没有匹配时返回零。原始循环的顺序由查询结果决定。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class First {
    private final BillMapper billMapper;

    First(BillMapper billMapper) {
        this.billMapper = billMapper;
    }

    int first(int accountId) {
        BillRow match = null;
        for (BillRow row : billMapper.selectRows(accountId)) {
            if (row.amount > 0) {
                match = row;
                break;
            }
        }
        if (match != null) return match.amount;
        return 0;
    }
}

interface BillMapper {
    @Select("""
        SELECT amount
        FROM bill
        WHERE account_id = #{accountId}
        ORDER BY amount
    """)
    List<BillRow> selectRows(int accountId);
}

class BillRow {
    int amount;
}
```

## AI 预处理结果

AI 将 Mapper 查询和未知次数的首项循环分别交付为关系语义，保留 Java 的匹配判断、字段读取和默认返回。

```diff
--- a/src/First.java
+++ b/src/First.java
@@ -1,36 +1,25 @@
 import java.util.List;
-import org.apache.ibatis.annotations.Select;

 class First {
-    private final BillMapper billMapper;
-
-    First(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
     int first(int accountId) {
-        BillRow match = null;
-        for (BillRow row : billMapper.selectRows(accountId)) {
-            if (row.amount > 0) {
-                match = row;
-                break;
-            }
-        }
+        List<BillRow> rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId
+               yield { amount: b.amount }
+        */
+        BillRow match;
+        /* @normalizer-ir
+           match = first from rows as row
+               where row.amount > 0
+               order by row.amount
+               yield row else null
+        */
         if (match != null) return match.amount;
         return 0;
     }
 }

-interface BillMapper {
-    @Select("""
-        SELECT amount
-        FROM bill
-        WHERE account_id = #{accountId}
-        ORDER BY amount
-    """)
-    List<BillRow> selectRows(int accountId);
-}
-
 class BillRow {
     int amount;
 }
```

预处理保留原始查询顺序，并显式交付 `order by`，因为 `first` 必须有确定的有序输入。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/First.java#First.first
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

E:
  bill:
    columns:
      account_id: number
      amount: number
    kind: table
    source: bill

R: |-
  COALESCE(
    (
      SELECT bill.amount
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.amount > 0
      ORDER BY bill.amount
      LIMIT 1
    ),
    0
  )
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 由 Mapper 和 `for` 循环完成查询、筛选和 `break`，new 在原位置分别交付关系来源和 `first` 关系语义。 | AI 只替换工程无法处理的未知次数集合循环，保留外围 Java 的入口和默认出口。 |
| old 在 Mapper SQL 中表达 `ORDER BY amount`，new 将同一排序带入 `first` 关系语义。 | 首项选择必须显式绑定顺序，不能把无序集合的任意元素冒充业务上的第一项。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只能看到循环遇到首个匹配后退出，new 用带筛选、排序和 `LIMIT 1` 的关系表达式保留同一语义。 | 集合首项、无匹配默认值和顺序约束在 PERS 中共同可见，内核不需要理解 Java `break`。 |
