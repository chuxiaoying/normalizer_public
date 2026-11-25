# Hello World：从另一个引用读回刚写的额度

[十个业务边界案例](README.md)

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

在 `Account.quota` 上用 `@EntryState` 标记状态，在形参 `observer` 上用 `@Nullable` 声明允许传入空值。方法体保留先写入额度、再解引用的顺序。

```diff
--- a/src/Account.java
+++ b/src/Account.java
@@ -1,5 +1,8 @@
+import normalizer.annotations.EntryState;
+
 class Quota { int units; }
 class Account {
+    @EntryState
     Quota quota;
-    int reset(Quota observer) {
+    int reset(@normalizer.annotations.Nullable Quota observer) {
         quota.units = 7;
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Quota:
    fields: { units: number }
entry:
  function: src/Account.java#Account.reset
  returns: number
P:
  parameters:
    observer: { position: 0, type: 'Quota?' }
E:
  quota: { kind: state, source: src/Account.java#Account.quota, type: Quota }
S:
  - write: { target: quota.units, value: '7' }
R: |-
  CASE
    WHEN observer == quota THEN 7
    ELSE observer.units
  END
throws:
  - { type: java.lang.NullPointerException, when: observer == null }
```

公式中的 `observer == quota` 比较两个引用是否指向同一个对象。

**判别例**　两个对象原本都为 3 时，传 quota 返回 7，传另一个对象返回 3。传 null 会抛异常，此时 quota.units 仍为 7。

**顺序对照**　把方法体改为先读原值，再写额度，最后按对象身份决定返回值：

    int prior = observer.units;
    quota.units = 7;
    return observer == quota ? 7 : prior;

同一对象、不同对象两种正常返回和上例相同；传 null 也仍抛同一种异常，但这次写入没有执行，额度保留原值 3。因此验收不能只对比 R 和异常类型：上例的 S 在 null 路径上也有写入，对照版本的 S 只允许在 observer 非空的路径上写入。
