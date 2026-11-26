# Hello World：集合筛选后统计正数

## 设计意图

对照含正、零、负与空集合，检验筛选必须先于计数。输入次序不改变同一过滤的计数；改变严格边界或过滤谓词可改变 R。

## 输入原始代码

入口从账单表取得行，返回金额大于零的账单数量。筛选条件和计数都属于集合语义。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class PositiveCount {
    private final BillMapper billMapper;

    PositiveCount(BillMapper billMapper) {
        this.billMapper = billMapper;
    }

    int count(int accountId) {
        int count = 0;
        for (BillRow row : billMapper.selectRows(accountId)) {
            if (row.amount > 0) {
                count++;
            }
        }
        return count;
    }
}

interface BillMapper {
    @Select("""
        SELECT amount
        FROM bill
        WHERE account_id = #{accountId}
    """)
    List<BillRow> selectRows(int accountId);
}

class BillRow {
    int amount;
}
```

## AI 预处理结果

循环本身的次数取决于查询结果。AI 保留 Java 方法和账单字段，在查询与循环原处交付局部关系 IR。Normalizer 继续解析外围 Java，并由内核处理筛选结果。

```diff
--- a/src/PositiveCount.java
+++ b/src/PositiveCount.java
@@ -1,33 +1,23 @@
 import java.util.List;
-import org.apache.ibatis.annotations.Select;

 class PositiveCount {
-    private final BillMapper billMapper;
-
-    PositiveCount(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
     int count(int accountId) {
         int count = 0;
-        for (BillRow row : billMapper.selectRows(accountId)) {
-            if (row.amount > 0) {
-                count++;
-            }
-        }
+        List<BillRow> rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId
+               yield { amount: b.amount }
+        */
+        /* @normalizer-ir
+           count += sum from rows as row
+               where row.amount > 0
+               yield 1
+        */
         return count;
     }
 }

-interface BillMapper {
-    @Select("""
-        SELECT amount
-        FROM bill
-        WHERE account_id = #{accountId}
-    """)
-    List<BillRow> selectRows(int accountId);
-}
-
 class BillRow {
     int amount;
 }
```

预处理只替换无法直接按有限次数展开的查询和循环片段，保留入口参数、初值和返回语句。集合的空集贡献零。

**预处理要点**

- AI 只补充循环片段的集合语义，复用已有关系聚合合同，工程面对保持 Java 合法的外围程序。
- 预处理改动范围应与工程无法处理的语义片段重合，不能把完整入口翻译成另一种语言。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/PositiveCount.java#PositiveCount.count
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
    SUM(
      SELECT 1
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.amount > 0
    ),
    0
  )
```

**PERS 验收要点**

- 集合主题先明确元素保留规则，再复用统一聚合处理空集、顺序和计数。
- 语言集合写法、库查询和业务集合语义分层，Java、JavaScript、TypeScript 可以共享同一类设计主张。
