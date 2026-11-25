# Hello World：从另一个引用读回刚写的额度

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
R: CASE WHEN observer == quota THEN 7 ELSE observer.units END
throws:
  - { type: java.lang.NullPointerException, when: observer == null }
```

公式中的 `observer == quota` 比较两个引用是否指向同一个对象。

**判别例**　两个对象原本都为 3 时，传 quota 返回 7，传另一个对象返回 3。传 null 会抛异常，此时 quota.units 仍为 7。
