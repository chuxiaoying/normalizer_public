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

AI 将 Mapper 查询和未知次数的首项循环分别交付为关系语义，保留 Java 的匹配判断、字段读取和默认返回。查询行用工程内行类型数组承载；去掉 Mapper 后以静态方法作为分析入口。

```diff
--- a/src/First.java
+++ b/src/First.java
@@ -1,36 +1,25 @@
-import java.util.List;
-import org.apache.ibatis.annotations.Select;
+import normalizer.intrinsics.N;

 class First {
-    private final BillMapper billMapper;
-
-    First(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
-    int first(int accountId) {
-        BillRow match = null;
-        for (BillRow row : billMapper.selectRows(accountId)) {
-            if (row.amount > 0) {
-                match = row;
-                break;
-            }
-        }
+    static int first(int accountId) {
+        BillRow[] rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId
+               yield { amount: b.amount }
+        */
+        @N.Nullable BillRow match;
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
    FIRST(bill.amount
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.amount > 0
      ORDER BY bill.amount
    ),
    0
  )
```

**预处理要点**

- AI 只替换工程无法处理的未知次数集合循环，保留外围 Java 的入口和默认出口。
- 首项选择必须显式绑定顺序，不能把无序集合的任意元素冒充业务上的第一项。

**PERS 验收要点**：集合首项、无匹配默认值和顺序约束在 PERS 中共同可见；未知次数的原循环由局部指令表达，不表示普通 Java 的 `break` 不受支持。

派生的“找到后加一”对照只在满足过滤条件的记录存在时读取首项，再计算 `amount + 1`；没有符合记录时仍返回 0，不是对空集默认值加一。首项的投影、过滤和 `ORDER BY` 必须都留在 `FIRST` 内，外层 CASE 保留读值的取得域。

存在性保护必须指向 FIRST 实际使用的同一批行、同一筛选与取得身份；另一张表或同表另一次取得都不是证据。一行的投影值可以为 null，FIRST 的值或 `COUNT(value)` 不能替代 `COUNT(*)` 对实际行数的观察。

排序证书来自实际的顺序节点：普通复合查询调用即使恰好也带有「关系、数字」两个参数，不代表已经排序。同一关系的普通向量调用不等于双键排序；双键排序必须保持两个键的优先级。

FIRST 的缺省值可以是精确表达式：例如 `2 + 3` 化为 5。只有筛选后的有序结果没有首项时才取 5；源表有记录但全被筛掉，也走这条路径。若首项实际存在且投影为 null，则保留 null，不取缺省。归约缺省值不改变来源、投影和观察范围。
