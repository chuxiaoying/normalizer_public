# Hello World：有序集合中的首个匹配

## 设计意图

检验有序集合的过滤、排序、首项和默认的关系。空集、不命中、多个命中须区分；只对存在路径读取首项，改变比较顺序可能改变所选元素。

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

**预处理要点**

- AI 只替换工程无法处理的未知次数集合循环，保留外围 Java 的入口和默认出口。
- 首项选择必须显式绑定顺序，不能把无序集合的任意元素冒充业务上的第一项。

**PERS 验收要点**：集合首项、无匹配默认值和顺序约束在 PERS 中共同可见，内核不需要理解 Java `break`。

派生的“找到后加一”对照只在满足过滤条件的记录存在时读取首项，再计算 `amount + 1`；没有符合记录时仍返回 0，不是对空集默认值加一。首项的投影、过滤和 `ORDER BY` 必须都留在 `FIRST` 内，外层 CASE 保留读值的取得域。

内部对照补充两个读值边界：相同关系的 EXISTS 可以给 FIRST 安全地选择空表默认值，另一关系的 EXISTS 不行；FIRST 和按非空选值计数（COUNT(value)）均不能冒充裸 COUNT(*) 的整表基数证据。直接传入内核关系节点，避免源前端已就查询类型区分而无法再观察这一步。

排序证书来自实际的顺序节点：普通复合查询调用即使恰好也带有「关系、数字」两个参数，不代表已经排序。内核单测以同一关系对照一般向量调用与明确的双键排序，后者必须保持两把键的次序。

FIRST 的空表缺省值可以是精确表达式：例如 `2+3` 归约为 5。此重建不改变关系、选中的列或真正选中 null 时的含义；只有空表使用 5。内核单测直接对照重建前后的聚合结构。
