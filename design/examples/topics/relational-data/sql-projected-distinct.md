# Hello World：跨两批账单统计不同账户

## 设计意图

检验先选列再跨来源去重；只交换已证明对称的并集来源。相同行的不同账户／同账户的不同账单不能都按完整记录去重，改变投影列确实应改变计数。

[本主题](README.md)

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

保留入口标识和结果类型。将仅声明查询的 mapper 接口改成闭合的规范载体，分别以已声明的非空整数行数组读取两表；再交付按 account_id 投影的去重并集计数。对非空列，COUNT(DISTINCT column) 在 UNION 后计数与原查询相同，这不是整张账单去重。没有绑定参数、写入或其他可观察的数据库效果。

```diff
--- a/src/BillReport.java
+++ b/src/BillReport.java
@@ -1,12 +1,12 @@
-import org.apache.ibatis.annotations.Select;
-interface BillReport {
-    @Select("""
-        SELECT COUNT(DISTINCT account_id)
-        FROM (
-            SELECT bill_id, account_id FROM pending_bill
-            UNION ALL
-            SELECT bill_id, account_id FROM posted_bill
-        ) AS all_bills
-        """)
-    long countAccounts();
+class AccountRow { int accountId; }
+class BillReport {
+    static long countAccounts() {
+        AccountRow[] pending;
+        /* @normalizer-ir pending = from pending_bill as p yield {accountId: p.account_id} */
+        AccountRow[] posted;
+        /* @normalizer-ir posted = from posted_bill as q yield {accountId: q.account_id} */
+        long count;
+        /* @normalizer-ir count = count distinct union pending.accountId, posted.accountId */
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


并集只针对已经类型闭合的候选列，按公开的表和列结构键稳定排列；交换两个来源不改变结果。重复同一分支时，剩下的单分支仍须 SELECT DISTINCT，不能把去重语义一并删除。换成 bill_id 时，公式和 E 中实际依赖的列都随之改变。
