# 有序 SQL 投影进入数组，再读取指定位置

## 设计意图

同一次查询交付的是已有次序的整份行数组。Java 按真实下标读取其中一行，再求它的可空字段。外部查询负责行的次序与内容，宿主负责下标和越界；不能取另一个位置、不能把选中行的 NULL 当成没有行，也不能把 Java 越界改成正常返回 null。

## 输入原始代码

```java
import java.util.List;

class MemoSlot {
    int id;
    int priority;
    String memo;
}

interface BillMapper {
    // SELECT id, priority, memo FROM bill WHERE account_id = :accountId
    // ORDER BY priority, id. bill.id is a non-null unique primary key.
    // The result is materialized in that order; memo may be SQL NULL.
    List<MemoSlot> read(int accountId);
}

class OrderedMemo {
    private final BillMapper bills;

    OrderedMemo(BillMapper bills) {
        this.bills = bills;
    }

    String atOffset(int accountId) {
        MemoSlot[] rows = bills.read(accountId).toArray(new MemoSlot[0]);
        return rows[1].memo;
    }
}
```

## AI 预处理结果

将 SQL 和列表物化交给局部关系投影；在投影中保留排序键、物理唯一键证据和 nullable 字段。宿主的 `rows[1].memo` 完全不改，内核负责执行。在此例中 `bill.id` 是来源合同给出的非空唯一主键，不是从名字猜测。

```diff
--- a/src/OrderedMemo.java
+++ b/src/OrderedMemo.java
@@ -1,27 +1,21 @@
-import java.util.List;
+import normalizer.intrinsics.N;

 class MemoSlot {
     int id;
     int priority;
-    String memo;
-}
-
-interface BillMapper {
-    // SELECT id, priority, memo FROM bill WHERE account_id = :accountId
-    // ORDER BY priority, id. bill.id is a non-null unique primary key.
-    // The result is materialized in that order; memo may be SQL NULL.
-    List<MemoSlot> read(int accountId);
+    @N.Nullable String memo;
 }

 class OrderedMemo {
-    private final BillMapper bills;
-
-    OrderedMemo(BillMapper bills) {
-        this.bills = bills;
-    }
-
-    String atOffset(int accountId) {
-        MemoSlot[] rows = bills.read(accountId).toArray(new MemoSlot[0]);
+    @N.Nullable
+    static String atOffset(int accountId) {
+        MemoSlot[] rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId
+               order by b.priority, b.id
+               yield { id: b.id, priority: b.priority, memo: b.memo }
+        */
         return rows[1].memo;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/OrderedMemo.java#OrderedMemo.atOffset, returns: 'text?' }
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill:
    kind: table
    source: bill
    columns: { account_id: number, id: number, priority: number, memo: 'text?' }
R: (SELECT bill.memo FROM bill WHERE bill.account_id == accountId ORDER BY bill.priority, bill.id)[1]
throws:
  - type: java.lang.ArrayIndexOutOfBoundsException
    when: |-
      COALESCE(
        SUM(
          SELECT 1
          FROM bill
          WHERE bill.account_id == accountId
        ),
        0
      ) <= 1
```

非零下标保留在整个有序投影之后。下标 0 的正常值按有序序列规范写为 FIRST，并仍单列空集越界；负下标是业务越界，不是假缺省；未交付来源次序的准备稿不应猜测哪一行位于指定下标。

**判别输入**：账号 7 有两行：`(id=1, priority=5, memo=null)`、`(id=2, priority=1, memo="A")`。按 priority 再按 id 排序时，下标 1 正常返回 null；把 id 提到第一排序键时，下标 1 正常返回 "A"。只保留一行或没有行时是越界，而不是这两种正常值之一。输入中其他账号的行不进入本账号数组。
