# Hello World：外部登记与通知的调用顺序

## 设计意图

检验登记、实际调用与后续通知的因果顺序。改成仅登记或条件未触发时不能出现通知；重复派发保留每次调用，规范排序不能改变 S。

## 输入原始代码

同步登记和通知是两次声明为 `void` 的外部操作，本例两者均正常完成。入参是整数 `value`，并未传入 Java 回调对象。观察调用顺序及正常返回；延迟派发另见 frontend-deferred-listener。每次操作都有自己的完成回执，即使结果中不读取回执，也不能删除或互换调用。

```java
class NotifyService {
    private final Notifier notifier;

    NotifyService(Notifier notifier) {
        this.notifier = notifier;
    }
    int send(int value) {
        notifier.register(value);
        notifier.accept(value);
        return value + 1;
    }
}
interface Notifier {
    void register(int value);
    void accept(int value);
}
```

## AI 预处理结果

```diff
--- a/src/NotifyService.java
+++ b/src/NotifyService.java
@@ -1,4 +1,7 @@
+import normalizer.annotations.External;
+
 class NotifyService {
+    @External
     private final Notifier notifier;

     NotifyService(Notifier notifier) {
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/NotifyService.java#NotifyService.send, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
S:
  - call: { external: notifier.register, args: [value], returns: void, result: $e_0 }
  - call: { external: notifier.accept, args: [value], returns: void, result: $e_1 }
R: value + 1
```
