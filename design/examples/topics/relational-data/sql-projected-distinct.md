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

保留入口标识和结果类型。将仅声明查询的 mapper 接口改成闭合的规范载体，以两个非空整数行数组交付原 SQL 语句在同一读取边界内取得的两批行，再交付按 account_id 投影的去重并集计数。两个局部投影不是先后另发两次查询。对非空列，COUNT(DISTINCT column) 在 UNION 后计数与原查询相同，这不是整张账单去重。没有绑定参数、写入或其他可观察的数据库效果。

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

同一次查询中的两支都只取账户编号。账单号对计数没有影响，可以从最终的表字段声明与公式中一起消去。

**判别例**　两表分别含 (101,7)、(202,7)，整行去重有两条，不同账户只有一个。把第二个账户改为 8，结果才变为二。两表均空时为零。


并集只针对已经类型闭合的候选列，按公开的表和列结构键稳定排列；交换两个来源不改变结果。重复同一分支时，剩下的单分支仍须 SELECT DISTINCT，不能把去重语义一并删除。换成 bill_id 时，公式和 E 中实际依赖的列都随之改变。

**来源合同对照**　并集中的每一侧都要先绑定到已声明的表行。写成一个从未绑定的本地名字，与已绑定但带过滤的表行，是两种不同的问题：前者没有已取得的来源，不能输出 PERS；后者读的是经过过滤的不同候选域，不得沿用无过滤的预期。预处理必须保留过滤，不能去掉 WHERE 使它看起来符合本例。

**同表不同列**　如果一项业务统计把两种合法的账户编号列也纳入候选，同一张表可以贡献两个不同的列来源。并集分支的等价要核对已取得的数据、表列与投影，不能只看表名或列名。对同一份已取得的数据，重复投影同一列不改变结果。源码的并集指令必须显式交付至少两个分支，只写一个由入口拒绝；写出两个相同分支后在内核归约为一个，是合法且仍保有去重的情况。用待入账表的 `account_id`、`alternate_account_id` 与已入账表的 `account_id` 为例：前两列在 E 中都要声明。两列分别为 `7`、`9`，已入账列只有 `7` 时，不同账户数是 2。颠倒三列的交付顺序或再写一次待入账表的 `account_id`，都应得到同一份完整 PERS。

**列类型一致性**　两个局部投影可以用不同载体和字段名，但只要来自同一个物理表列，就必须给出相同的业务类型。若把 `pending_bill.account_id` 同时声明成数字和文本，分析应拒绝并指出冲突的列，即使文本载体没有出现在最终并集；不能让后一份声明覆盖前一份。反过来，同一个已取得的列用两个局部载体引用，仍是一个并集分支，不新增 E。若真把某一列在两个时点各读一遍，即使同表、同列、同类型，也不能合并取得。比如两次读到的账户集合分别为 `{7}`、`{9}`，并集计数是 2，不能因列名相同去掉一支。按 [RA-08](../../../chapters/05-关系代数.md#ra-08-取得后的化简不搬动读写) 保留真实读取边界。
