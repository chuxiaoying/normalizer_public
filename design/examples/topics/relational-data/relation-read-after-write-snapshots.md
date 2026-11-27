# 关系快照：写前与写后不是同一次读取

## 设计意图

验证同一张表在写入前后取得的两份行：相同的表名、筛选和列合同不构成相同的取得，不能只按查询外观约掉。读取已取得的旧行不是再次访问数据库。

## 输入原始代码

入口接到账号后先查账单，再按该账号原子删除，成功后重新查询。本例无触发器或跨表级联，不假定事务隔离或不会有并发更新。数据库失败沿原位置抛出，原 SQL 合同不允许在失败时提交部分删除。原查询在返回时已经物化当前行。

```java
import java.util.List;

class DbFailure extends RuntimeException {}
class SnapshotRow { int accountId; }

interface BillMapper {
    // SELECT account_id FROM bill WHERE account_id = :accountId.
    // Returns a materialized list on each invocation.
    List<SnapshotRow> selectForAccount(int accountId);

    // Atomic DELETE FROM bill WHERE account_id = :accountId.
    // No trigger/cascade. If it fails, no rows are deleted by this statement.
    int deleteForAccount(int accountId) throws DbFailure;
}

class Snapshot {
    private final BillMapper bills;

    Snapshot(BillMapper bills) {
        this.bills = bills;
    }

    int delta(int accountId) throws DbFailure {
        List<SnapshotRow> oldRows = bills.selectForAccount(accountId);
        int before = oldRows.size();
        bills.deleteForAccount(accountId);
        List<SnapshotRow> newRows = bills.selectForAccount(accountId);
        int after = newRows.size();
        return before - after;
    }
}
```

## AI 预处理结果

移除只负责宿主装配的 Mapper，把三次数据库动作分别放回原位置：读表、原子写、再读表。用工程内有类型的行载体和局部 SQL 指令交付，不把回执装成入口参数，也不把 Java 对两份计数的相减提前算好。

```diff
--- a/src/Snapshot.java
+++ b/src/Snapshot.java
@@ -1,31 +1,17 @@
-import java.util.List;
-
 class DbFailure extends RuntimeException {}
 class SnapshotRow { int accountId; }
-
-interface BillMapper {
-    // SELECT account_id FROM bill WHERE account_id = :accountId.
-    // Returns a materialized list on each invocation.
-    List<SnapshotRow> selectForAccount(int accountId);
-
-    // Atomic DELETE FROM bill WHERE account_id = :accountId.
-    // No trigger/cascade. If it fails, no rows are deleted by this statement.
-    int deleteForAccount(int accountId) throws DbFailure;
-}
-
 class Snapshot {
-    private final BillMapper bills;
-
-    Snapshot(BillMapper bills) {
-        this.bills = bills;
-    }
-
-    int delta(int accountId) throws DbFailure {
-        List<SnapshotRow> oldRows = bills.selectForAccount(accountId);
-        int before = oldRows.size();
-        bills.deleteForAccount(accountId);
-        List<SnapshotRow> newRows = bills.selectForAccount(accountId);
-        int after = newRows.size();
+    static int delta(int accountId) throws DbFailure {
+        SnapshotRow[] oldRows;
+        /* @normalizer-ir oldRows = from bill as b where b.account_id == accountId yield { accountId: b.account_id } */
+        int before;
+        /* @normalizer-ir before = count from oldRows as r */
+        int written;
+        /* @normalizer-ir written = delete from bill as b where b.account_id == accountId returning affected_rows atomic throws DbFailure */
+        SnapshotRow[] newRows;
+        /* @normalizer-ir newRows = from bill as b where b.account_id == accountId yield { accountId: b.account_id } */
+        int after;
+        /* @normalizer-ir after = count from newRows as r */
         return before - after;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry:
  function: src/Snapshot.java#Snapshot.delta
  returns: number
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill:
    kind: table
    source: bill
    columns: { account_id: number }
  $a_0:
    kind: acquisition
    source: bill
    when: completion.is_return($e_0)
    after:
      - result: $e_0
R: |-
  COALESCE(
    SUM(
      SELECT 1
      FROM bill
      WHERE bill.account_id == accountId
    ),
    0
  ) - COALESCE(
    SUM(
      SELECT 1
      FROM $a_0 AS bill
      WHERE bill.account_id == accountId
    ),
    0
  )
S:
  - sql: |-
      DELETE FROM bill
      WHERE bill.account_id = accountId
    result: $e_0
    returns: number
throws:
  - type: DbFailure
    when: completion.is_throw($e_0, DbFailure)
```

## 判别用例

- 删除成功：若删除前有一条匹配账单，且期间没有其他修改，结果由 1 变成 0，差为 1。该具体例不构成计算 `before - after` 等价于受影响行数的全域证明。
- 删除失败：写入尝试已发生，记录其回执和 `DbFailure`。其后没有重新读取；`$a_0` 只在写成功域内存在。
- 复用旧行：若把第二次查库换成计数已取得的 `oldRows`，差恒为 0；写动作及其失败合同仍在。不能生成新的取得。
- 改读其他表：改为从 `archived_bill` 取得后一个计数时，其物理来源独立，没有来自 `bill` 写入的同表依赖。
- 条件删除：不删路径仍执行真实的后续读，删除路径要等删除成功；失败不执行该读取。汇合点的一次读取只生成一个取得，关联回执只在实际删除成功的路径成立。
