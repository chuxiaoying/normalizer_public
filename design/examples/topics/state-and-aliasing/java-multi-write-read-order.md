# Hello World：多次写入与不同读取时点

## 输入原始代码

入口先读取旧余额，再写入新余额，最后用新旧两次读取计算差额。

```java
class Ledger {
    int balance;

    int delta(int amount) {
        int before = balance;
        balance = balance + amount;
        return balance - before;
    }
}
```

## AI 预处理结果

```diff
--- a/src/Ledger.java
+++ b/src/Ledger.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.EntryState;
+
 class Ledger {
+    @EntryState
     int balance;
```

预处理只标记已有状态，保留读取、写入和返回顺序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Ledger.java#Ledger.delta
  returns: number

P:
  parameters:
    amount: { position: 0, type: number }

E:
  balance:
    kind: state
    source: src/Ledger.java#Ledger.balance
    type: number

S:
  - write:
      target: balance
      value: balance + amount

R: amount
```

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只按源码顺序读取、赋值和再读取；new 在 E 中保留旧状态，在 S 中记录新值，并把两次读取的关系化简为 `amount`。 | 状态写入与读取时点共同决定结果。不能只保留最终字段值，也不能把写入前后的两个观察合成一个快照。 |

原例是单次写入的对照。`read-between-two-writes` 在第一次写后读取中间余额，再写一次。余额为 10、amount 为 3 时，S 依次写入 13 和 14，R 为 1；不能合并成一次写入。
