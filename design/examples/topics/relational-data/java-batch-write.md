# Hello World：关系批量写入

## 输入原始代码

amount 是入口参数，不是表列。仅当 amount 大于零才发起一次原子的多行 UPDATE；更新命中账户的全部行。失败时整条语句不提交并传播 DatabaseWriteError；受影响行数不作为方法返回值。

```java
import org.apache.ibatis.annotations.Param;
import org.apache.ibatis.annotations.Update;

class DatabaseWriteError extends RuntimeException {}

class PayableService {
    private final PayableMapper mapper;
    PayableService(PayableMapper mapper) { this.mapper = mapper; }

    int close(int accountId, int amount) throws DatabaseWriteError {
        if (amount > 0) mapper.close(accountId, amount);
        return amount;
    }
}
interface PayableMapper {
    @Update("""
        UPDATE payable
        SET closed_amount = #{amount}, status = 'CLOSED'
        WHERE account_id = #{accountId}
    """)
    int close(@Param("accountId") int accountId, @Param("amount") int amount) throws DatabaseWriteError;
}
```

## AI 预处理结果

```diff
--- a/src/PayableService.java
+++ b/src/PayableService.java
@@ -1,22 +1,18 @@
-import org.apache.ibatis.annotations.Param;
-import org.apache.ibatis.annotations.Update;
-
 class DatabaseWriteError extends RuntimeException {}

 class PayableService {
-    private final PayableMapper mapper;
-    PayableService(PayableMapper mapper) { this.mapper = mapper; }
-
     int close(int accountId, int amount) throws DatabaseWriteError {
-        if (amount > 0) mapper.close(accountId, amount);
+        if (amount > 0) {
+            int affected;
+            /* @normalizer-ir
+               affected = update payable as p
+                   set { closed_amount: amount, status: "CLOSED" }
+                   where p.account_id == accountId
+                   returning affected_rows
+                   atomic
+                   throws DatabaseWriteError
+            */
+        }
         return amount;
     }
 }
-interface PayableMapper {
-    @Update("""
-        UPDATE payable
-        SET closed_amount = #{amount}, status = 'CLOSED'
-        WHERE account_id = #{accountId}
-    """)
-    int close(@Param("accountId") int accountId, @Param("amount") int amount) throws DatabaseWriteError;
-}
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/PayableService.java#PayableService.close, returns: number }
P:
  parameters:
    accountId: { position: 0, type: number }
    amount: { position: 1, type: number }
E:
  payable:
    kind: table
    source: payable
    columns: { account_id: number, closed_amount: number, status: text }
S:
  - sql: |-
      UPDATE payable
      SET closed_amount = amount, status = 'CLOSED'
      WHERE payable.account_id = accountId
    returns: number
    result: $e_0
    when: amount > 0
R: amount
throws:
  - { type: DatabaseWriteError, when: 'amount > 0 AND completion.is_throw($e_0, DatabaseWriteError)' }
```

## 两条独立语句的部分失败

分别执行记账 INSERT 和状态 UPDATE。每条语句各自原子，但入口没有外层事务，也没有补偿操作。第一条失败时第二条不发生；第一条已提交而第二条失败时，第一条的写入保留。正常返回第二条语句影响的行数。

```java
class AuditInsertError extends RuntimeException {}
class CloseUpdateError extends RuntimeException {}

class PayableAudit {
    int closeWithAudit(int accountId, int amount)
        throws AuditInsertError, CloseUpdateError {
        int inserted;
        /* @normalizer-ir
           inserted = insert into payable_log
               values { account_id: accountId, amount: amount }
               returning affected_rows
               atomic
               throws AuditInsertError
        */
        int affected;
        /* @normalizer-ir
           affected = update payable as p
               set { status: "CLOSED" }
               where p.account_id == accountId
               returning affected_rows
               atomic
               throws CloseUpdateError
        */
        return affected;
    }
}
```

源码已经给出局部关系语义，预处理不合并两条写入，不替外层补事务。

```yaml
schema: normalizer.pers/1
entry: { function: src/PayableAudit.java#PayableAudit.closeWithAudit, returns: number }
P:
  parameters:
    accountId: { position: 0, type: number }
    amount: { position: 1, type: number }
E:
  payable_log:
    kind: table
    source: payable_log
    columns: { account_id: number, amount: number }
  payable:
    kind: table
    source: payable
    columns: { account_id: number, status: text }
S:
  - sql: |-
      INSERT INTO payable_log (account_id, amount)
      VALUES (accountId, amount)
    returns: number
    result: $e_0
  - sql: |-
      UPDATE payable
      SET status = 'CLOSED'
      WHERE payable.account_id = accountId
    returns: number
    result: $e_1
    when: completion.is_return($e_0)
R: $e_1
throws:
  - type: AuditInsertError
    when: completion.is_throw($e_0, AuditInsertError)
  - type: CloseUpdateError
    when: completion.is_return($e_0) AND completion.is_throw($e_1, CloseUpdateError)
```

变体只改变第二条语句的状态值。第一条写入、执行顺序和两个失败域均不变。
