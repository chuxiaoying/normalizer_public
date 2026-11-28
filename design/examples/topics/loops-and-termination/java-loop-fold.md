# Hello World：for-each 折叠为关系聚合

## 设计意图

检验 for-each 的逐行观察到关系聚合的等价性。空集合取初值，非空集合按真实参与项累积；修改过滤或投影必须改变相应依赖，不能只看循环初值。

## 输入原始代码

这个案例展示 AI 预处理如何用一次求和替换 `for-each`。循环从 Mapper 调用取得账单，在服务方法内只筛选并累加正金额。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class ArrearsService {
    private final BillMapper billMapper;

    ArrearsService(BillMapper billMapper) {
        this.billMapper = billMapper;
    }

    int positiveTotal(int accountId) {
        int total = 0;

        for (BillRow row : billMapper.selectOpenBills(accountId)) {
            if (row.amount > 0) {
                total += row.amount;
            }
        }

        return total;
    }
}

interface BillMapper {
    @Select("""
        SELECT account_id AS accountId,
               amount,
               status
        FROM bill
        WHERE account_id = #{accountId}
          AND status = 'OPEN'
    """)
    List<BillRow> selectOpenBills(int accountId);
}

class BillRow {
    int accountId;
    int amount;
    String status;
}
```

源码中的 `for` 具有明确的集合折叠语义。Mapper 查询是只读关系来源，数据库驱动和 Mapper 实现不属于本案例观察范围。

## AI 预处理结果

AI 将原 SQL 的账户筛选、状态筛选和字段投影绑定到查询位置的 `rows`。循环位置只描述对这些行的正金额累加。

分析代码以 `ArrearsService.INSTANCE` 定位同一个入口方法；Mapper 调用在原观察处交为关系读取，行数组只承载同一批行。

```diff
--- a/src/ArrearsService.java
+++ b/src/ArrearsService.java
@@ -1,40 +1,25 @@
-import java.util.List;
-import org.apache.ibatis.annotations.Select;
-
 class ArrearsService {
-    private final BillMapper billMapper;
-
-    ArrearsService(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
+    static final ArrearsService INSTANCE = new ArrearsService();

     int positiveTotal(int accountId) {
         int total = 0;
-
-        for (BillRow row : billMapper.selectOpenBills(accountId)) {
-            if (row.amount > 0) {
-                total += row.amount;
-            }
-        }
-
+        BillRow[] rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId and b.status == "OPEN"
+               yield { accountId: b.account_id, amount: b.amount, status: b.status }
+        */
+        /* @normalizer-ir
+           total += sum from rows as row
+               where row.amount > 0
+               yield row.amount
+        */
         return total;
     }
-}
-
-interface BillMapper {
-    @Select("""
-        SELECT account_id AS accountId,
-               amount,
-               status
-        FROM bill
-        WHERE account_id = #{accountId}
-          AND status = 'OPEN'
-    """)
-    List<BillRow> selectOpenBills(int accountId);
 }

 class BillRow {
     int accountId;
     int amount;
     String status;
 }
```

求和在空集上贡献零。内核通过 `rows` 合并两处筛选，再结合 Java 中的初值和返回语句推导 `R`。

**预处理要点**：AI 负责闭合 SQL 与未知次数循环各自的语义；语言解析器读取保留的 Java，内核依据交付的语义组合初值、局部结果与返回值。不能为迁就现有输入能力，把数据库查询结果改成新的入口参数。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/ArrearsService.java#ArrearsService.INSTANCE.positiveTotal
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

E:
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      amount: number
      status: text

R: |-
  COALESCE(
    SUM(
      SELECT bill.amount
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.status == "OPEN"
        AND bill.amount > 0
    ),
    0
  )
```

这个案例没有 `S`。只读关系进入 `E` 和 `R`，`for-each` 的执行次数不会进入内核执行路径。

**PERS 验收要点**

- 数据来源应保持业务身份。数据库表不能因实现适配需要而变成调用参数或内存状态；可靠的来源绑定是完整归一化的前提。
- 已有集合求和能力需要与真实业务来源接通。同一筛选与聚合规则应不受 SQL、循环或集合链的实现分工影响；把 `amount > 0` 移到 SQL 应保持业务表达，改成 `amount >= 10` 才改变规则。

**整数区间对照。** 跳过小于 1 的索引、在索引大于 300 时结束，只改变参与项的区间，不需要保留循环状态。

```diff
 class RangeFold {
     static int total(int hi) {
-        int total = 0;
-        for (int i = 0; i < hi; i++) {
-            if (i < 1) continue;
-            if (i > 300) break;
-            total += i;
-        }
+        int total;
+        /* @normalizer-ir total = sum from [1, min(hi, 301)) */
         return total;
     }
 }
```

`hi = 1` 时为空，得 0；`hi = 2` 得 1；`hi = 301` 及更大的值都得 45150。内核不能为了化简而枚举上界之前的每个前缀，也不能因为旧指令的枚举预算而拒绝这个区间。直接求和与无过滤的恒等投影同形。另一个变体 `sum from [1, 4) as i yield i * i` 得 14，不得当作恒等投影。

**保留精确聚合。** 符号区间 `[1, hi)` 中只加 `i > threshold` 的平方时，保留筛选；`hi = 5, threshold = 2` 得 25。对每项取三次方时不套二次化简，保留精确聚合；`hi = 4` 得 36，空区间得 0。

**二次式的三类项都参与求和。** 对 `[-2, 4)` 上的 `i * i - 3 * i + 5` 求和，平方项合计 19，一次项合计 -9，常数项合计 30，结果为 40。将同一个投影应用到空区间 `[4, -2)`，三类项都没有取值，结果为 0。

在同一非空区间仅投影 `3 * i + 5` 时，一次项合计 9，常数项合计 30；没有平方项，结果为 39。
