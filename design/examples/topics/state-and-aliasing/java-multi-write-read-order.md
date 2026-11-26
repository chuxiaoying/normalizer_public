# Hello World：多次写入与不同读取时点

## 设计意图

区分第一次写前、两次写之间和最后一次写后的读值。满足条件才执行的写不能越过归并；等价反向条件保留写入时点，不把结果相同误成事件相同。局部别名写入后从分支提前返回，不再进入后续语句，也不因借用局部别名生成外部 S。

同一已声明数值状态的简单复合赋值 balance += amount 与显式读取后写回得到相同 E、S 和 R。

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
@@ -1,9 +1,16 @@
+import normalizer.intrinsics.N;
+
 class Ledger {
+    @N.EntryState
     int balance;

     int delta(int amount) {
         int before = balance;
         balance = balance + amount;
         return balance - before;
     }
 }
+
+class Beans {
+    static final Ledger ENTRY = new Ledger();
+}
```

预处理标记已有状态并显式装配接收者，保留读取、写入和返回顺序。

已装配实例由 `Beans.ENTRY` 定位。初始化只是装配，不替换这次入口分析时已声明的状态快照。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Ledger.java#Beans.ENTRY.delta
  returns: number

P:
  parameters:
    amount: { position: 0, type: number }

E:
  balance:
    kind: state
    source: src/Ledger.java#Beans.ENTRY.balance
    type: number

S:
  - write:
      target: balance
      value: balance + amount

R: amount
```

**PERS 验收要点**：状态写入与读取时点共同决定结果。不能只保留最终字段值，也不能把写入前后的两个观察合成一个快照。

原例是单次写入的对照。`read-between-two-writes` 在第一次写后读取中间余额，再写一次。余额为 10、amount 为 3 时，S 依次写入 13 和 14，R 为 1；不能合并成一次写入。

## 两条分支的写入与无写入路径

初始额度为 9 时，传入 `(5, true)` 写入 5 并返回 5；传入 `(0, true)` 不写入，仍返回 9；传入 `(5, false)` 写入并返回 2。预处理标记已有的额度状态并装配接收者，不能替无写入路径造一次写入。

```java
class RouteQuota { int units; }
class StoreRoute {
    RouteQuota quota;
    int next(int units, boolean replace) {
        if (replace) {
            if (units > 0) quota.units = units;
        } else {
            quota.units = 2;
        }
        return quota.units;
    }
}
```

```diff
--- a/src/StoreRoute.java
+++ b/src/StoreRoute.java
@@ -1,12 +1,22 @@
+import normalizer.intrinsics.N;
+
 class RouteQuota { int units; }
 class StoreRoute {
+    @N.EntryState
     RouteQuota quota;
     int next(int units, boolean replace) {
         if (replace) {
             if (units > 0) quota.units = units;
         } else {
             quota.units = 2;
         }
         return quota.units;
     }
 }
+
+class Beans {
+    static final StoreRoute ENTRY = new StoreRoute();
+    static {
+        ENTRY.quota = new RouteQuota();
+    }
+}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/StoreRoute.java#Beans.ENTRY.next, returns: number }
types:
  RouteQuota: { fields: { units: number } }
P:
  parameters:
    units: { position: 0, type: number }
    replace: { position: 1, type: bool }
E:
  quota: { kind: state, source: src/StoreRoute.java#Beans.ENTRY.quota, type: RouteQuota }
S:
  - write: { target: quota.units, value: units, when: replace and units > 0 }
  - write: { target: quota.units, value: '2', when: NOT (replace) }
R: |-
  CASE
    WHEN NOT (replace) THEN 2
    WHEN units <= 0 THEN quota.units
    ELSE units
  END
```

## 对照：分支内写入局部对象

同样的控制流写入的是本次调用新建的对象，不是外部状态。未写入时读到构造时的字段默认值 0。只发布返回值 R，不发布 S。无需预处理。

```java
class RouteBox { int amount; }
class StoreRoute {
    static int local(int amount, boolean replace) {
        RouteBox box = new RouteBox();
        if (replace) {
            if (amount > 0) box.amount = amount;
        } else {
            box.amount = 2;
        }
        return box.amount;
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/StoreRoute.java#StoreRoute.local, returns: number }
P:
  parameters:
    amount: { position: 0, type: number }
    replace: { position: 1, type: bool }
R: |-
  CASE
    WHEN NOT (replace) THEN 2
    WHEN amount > 0 THEN amount
    ELSE 0
  END
```

局部写入后提前返回的变体专门让该域返回已写入的 `amount + 1`；未命中时仍读取原结果。只有 `replace AND amount > 0` 生效域返回该值，不能把加一挪到其他路径，也不得为局部别名生成 S。
