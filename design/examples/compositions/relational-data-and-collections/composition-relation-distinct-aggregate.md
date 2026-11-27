# 关系投影进入集合后再去重计数

## 设计意图

一次 SQL 查询带回完整的值序列。Java 集合逻辑先排除 null，再按所选字段的值去重，最后统计。去重的是投影值，不是全行、物理账号，也不是查询出现了多少次；输入中的空字符串是已取得且非空的文本。

## 输入原始代码

```java
import java.util.HashSet;
import java.util.List;
import java.util.Set;

class MemoRow { String displayMemo; }

interface BillMapper {
    // SELECT memo AS displayMemo FROM bill WHERE account_id = :accountId.
    // The returned list preserves every matching row and SQL NULL.
    List<MemoRow> read(int accountId);
}

class DistinctMemo {
    private final BillMapper bills;

    DistinctMemo(BillMapper bills) {
        this.bills = bills;
    }

    int count(int accountId) {
        Set<String> values = new HashSet<>();
        for (MemoRow row : bills.read(accountId)) {
            if (row.displayMemo != null) values.add(row.displayMemo);
        }
        return values.size();
    }
}
```

## AI 预处理结果

保留原账号过滤和可空字段，明确写出按该字段执行的聚合。原程序先排除 null，而 SQL 的投影计数定义也跳过 NULL，所以可以交付 `count distinct from … yield expr`。若原程序让 HashSet 保存 null 并计作一个成员，则不能照搬该指令。无其他数据库动作，Mapper 只是装配，不变成业务 P。

```diff
--- a/src/DistinctMemo.java
+++ b/src/DistinctMemo.java
@@ -1,27 +1,17 @@
-import java.util.HashSet;
-import java.util.List;
-import java.util.Set;
+import normalizer.intrinsics.N;

-class MemoRow { String displayMemo; }
-
-interface BillMapper {
-    // SELECT memo AS displayMemo FROM bill WHERE account_id = :accountId.
-    // The returned list preserves every matching row and SQL NULL.
-    List<MemoRow> read(int accountId);
-}
+class MemoRow { @N.Nullable String displayMemo; }

 class DistinctMemo {
-    private final BillMapper bills;
-
-    DistinctMemo(BillMapper bills) {
-        this.bills = bills;
-    }
-
-    int count(int accountId) {
-        Set<String> values = new HashSet<>();
-        for (MemoRow row : bills.read(accountId)) {
-            if (row.displayMemo != null) values.add(row.displayMemo);
-        }
-        return values.size();
+    static int count(int accountId) {
+        MemoRow[] rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId
+               yield { displayMemo: b.memo }
+        */
+        int result;
+        /* @normalizer-ir result = count distinct from rows as row yield row.displayMemo */
+        return result;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/DistinctMemo.java#DistinctMemo.count, returns: number }
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill:
    kind: table
    source: bill
    columns: { account_id: number, memo: 'text?' }
R: SELECT COUNT(DISTINCT bill.memo) FROM bill WHERE bill.account_id == accountId
```

本账号对应投影 `[null, "A", "A", "", null]` 时，去重后计 2（`"A"` 与空字符串）；不去重但仍排除 null 计 3；换成另一物理列 `audit_memo` 后必须改读该列，不继续用 `memo` 的值。空集和只有 NULL 的集合都返回 0。
