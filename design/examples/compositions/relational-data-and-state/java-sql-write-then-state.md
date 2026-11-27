# 删除成功后，状态才累计删除行数

## 设计意图

按[关系代数](../../../chapters/关系代数.md)交付实际写入，并按[PERS登场](../../../chapters/PERS登场.md)保留同一次写入的完成和后续状态。写入失败时，不能用假想行数继续更新内存；写入成功后返回更新后的值，也不能误读入口快照。

## 输入原始代码

已装配的账单端口按账户删除已关闭账单，正常响应为实际删除的行数；操作具有原子性，失败抛出 DatabaseWriteError。入口已存在累计状态 removed。这里只观察删除、状态更新和返回。

```java
import normalizer.intrinsics.N;

class DatabaseWriteError extends RuntimeException {}
interface ClosedBills {
    int deleteClosed(int accountId) throws DatabaseWriteError;
}
class CleanupLog {
    @N.EntryState int removed;
    private final ClosedBills bills;

    CleanupLog(ClosedBills bills) { this.bills = bills; }

    int removeClosed(int accountId) {
        int affected = bills.deleteClosed(accountId);
        this.removed += affected;
        return this.removed;
    }
}
```

## AI 预处理结果

把已核实的 SQL 写入和完成直接交给关系指令，保留调用后的原生 += 和读取。显式根定位实例；保留 removed 的入口状态身份。

```diff
--- a/src/CleanupLog.java
+++ b/src/CleanupLog.java
@@ -1,18 +1,19 @@
 import normalizer.intrinsics.N;

 class DatabaseWriteError extends RuntimeException {}
-interface ClosedBills {
-    int deleteClosed(int accountId) throws DatabaseWriteError;
-}
 class CleanupLog {
+    static final CleanupLog INSTANCE = new CleanupLog();
     @N.EntryState int removed;
-    private final ClosedBills bills;
-
-    CleanupLog(ClosedBills bills) { this.bills = bills; }

     int removeClosed(int accountId) {
-        int affected = bills.deleteClosed(accountId);
+        int affected;
+        /* @normalizer-ir
+           affected = delete from bill as b
+               where b.account_id == accountId and b.status == "CLOSED"
+               returning affected_rows
+               atomic throws DatabaseWriteError
+        */
         this.removed += affected;
         return this.removed;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CleanupLog.java#CleanupLog.INSTANCE.removeClosed, returns: number }
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill: { kind: table, source: bill, columns: { account_id: number, status: text } }
  removed: { kind: state, source: src/CleanupLog.java#CleanupLog.INSTANCE.removed, type: number }
S:
  - result: $e_0
    returns: number
    sql: |-
      DELETE FROM bill
      WHERE bill.account_id = accountId
        AND bill.status = 'CLOSED'
  - write:
      target: removed
      value: removed + $e_0
      when: completion.is_return($e_0)
throws:
  - type: DatabaseWriteError
    when: completion.is_throw($e_0, DatabaseWriteError)
R: removed + $e_0
```

例如原值为 7、成功删除 2 行，写入和返回均为 9；删除失败时只记录尝试，正常 R 不成立，内存不写。对照先把原值存入局部变量后返回：成功时 R 为 7，但后续写入仍是 9，不能因为返回旧值就删写入。
