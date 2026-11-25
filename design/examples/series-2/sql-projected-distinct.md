# Hello World：跨两批账单统计不同账户

[十个业务边界案例](README.md)

## 输入原始代码

pending_bill 与 posted_bill 各有 bill_id 和非空 account_id。入口只读两张表，观察不同账户总数。

```java
import org.apache.ibatis.annotations.Select;
interface BillReport {
    @Select("""
        SELECT COUNT(DISTINCT account_id)
        FROM (
            SELECT bill_id, account_id FROM pending_bill
            UNION ALL
            SELECT bill_id, account_id FROM posted_bill
        ) AS all_bills
        """)
    long countAccounts();
}
```

## AI 预处理结果

保留方法签名，把 @Select 交给关系解析，形成两张表保留重复行的并集，再按 account_id 去重计数。这里没有绑定参数、写入或其他可观察的数据库效果。

```diff
--- a/src/BillReport.java
+++ b/src/BillReport.java
@@ -1,12 +1,14 @@
-import org.apache.ibatis.annotations.Select;
 interface BillReport {
-    @Select("""
-        SELECT COUNT(DISTINCT account_id)
-        FROM (
-            SELECT bill_id, account_id FROM pending_bill
-            UNION ALL
-            SELECT bill_id, account_id FROM posted_bill
-        ) AS all_bills
-        """)
-    long countAccounts();
+    default long countAccounts() {
+        long count;
+        /* @normalizer-ir
+           count = SELECT COUNT(DISTINCT account_id)
+             FROM (
+               SELECT bill_id, account_id FROM pending_bill
+               UNION ALL
+               SELECT bill_id, account_id FROM posted_bill
+             ) AS all_bills
+        */
+        return count;
+    }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry: { function: src/BillReport.java#BillReport.countAccounts, returns: number }
P: { parameters: {} }
E:
  pending_bill: { kind: table, source: pending_bill, columns: { account_id: number } }
  posted_bill: { kind: table, source: posted_bill, columns: { account_id: number } }
R: |-
  SELECT COUNT(*) FROM (
    SELECT account_id FROM pending_bill
    UNION
    SELECT account_id FROM posted_bill
  ) AS accounts
```

两次查询都只取账户编号。账单号对计数没有影响，可以从最终的表字段声明与公式中一起消去。

**判别例**　两表分别含 (101,7)、(202,7)，整行去重有两条，不同账户只有一个。把第二个账户改为 8，结果才变为二。两表均空时为零。
