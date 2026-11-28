# Hello World：从另一个引用读回刚写的额度

## 设计意图

对照同一对象的两个引用、不同对象以及可空观察者。先写后读时别名应看见新值；交换读写改变读值，先写之后发生空引用异常也不能抹掉已执行的 S。

[本主题](README.md)

## 输入原始代码

账户已有的 quota 非空。observer 可以为空，也可以是同一个 Quota 实例；字段为普通数值。

```java
class Quota { int units; }
class Account {
    Quota quota;
    int reset(Quota observer) {
        quota.units = 7;
        return observer.units;
    }
}
```

## AI 预处理结果

在 `Account.quota` 上用 `@N.EntryState` 标记状态，在形参 `observer` 上用 `@N.Nullable` 声明允许传入空值。方法体保留先写入额度、再解引用的顺序。

```diff
--- a/src/Account.java
+++ b/src/Account.java
@@ -1,8 +1,18 @@
+import normalizer.intrinsics.N;
+
 class Quota { int units; }
 class Account {
+    @N.EntryState
     Quota quota;
-    int reset(Quota observer) {
+    int reset(@N.Nullable Quota observer) {
         quota.units = 7;
         return observer.units;
     }
 }
+
+class Beans {
+    static final Account ENTRY = new Account();
+    static {
+        ENTRY.quota = new Quota();
+    }
+}
```

已装配实例由 `Beans.ENTRY` 定位。初始化只是装配，不替换这次入口分析时已声明的状态快照。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Quota:
    fields: { units: number }
entry:
  function: src/Account.java#Beans.ENTRY.reset
  returns: number
P:
  parameters:
    observer: { position: 0, type: 'Quota?' }
E:
  quota: { kind: state, source: src/Account.java#Beans.ENTRY.quota, fields: { units: number } }
S:
  - write: { target: quota.units, value: '7' }
R: |-
  CASE
    WHEN quota == observer THEN 7
    ELSE observer.units
  END
throws:
  - { type: java.lang.NullPointerException, when: observer == null }
```

正常完成域已知 `observer` 非空，两个操作数按同一个 `Quota` 类型比较其真实来源，入口快照引用的 `entry_snapshot` 在参数 `parameter` 前。因此 `quota == observer` 判定两者是否指向同一对象；空引用仍在独立的异常条件里。

**判别例**　两个对象原本都为 3 时，传 quota 返回 7，传另一个对象返回 3。传 null 会抛异常，此时 quota.units 仍为 7。

**顺序对照**　把方法体改为先读原值，再写额度，最后按对象身份决定返回值：

    int prior = observer.units;
    quota.units = 7;
    return observer == quota ? 7 : prior;

同一对象、不同对象两种正常返回和上例相同；传 null 也仍抛同一种异常，但这次写入没有执行，额度保留原值 3。因此验收不能只对比 R 和异常类型：上例的 S 在 null 路径上也有写入，对照版本的 S 只允许在 observer 非空的路径上写入。

**类型匹配对照**　写 7 后用 `try` 读 `observer.units`。捕获 `NullPointerException` 时，传 null 返回 handler 的 11，S 仍恰好写 7 一次；同一引用仍返回 7，另一个初值为 3 的对象仍返回 3。只捕获无关的 `IndexOutOfBoundsException` 时，null 仍抛出，不执行该 handler。
