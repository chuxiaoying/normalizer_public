# Hello World：外部调用超时

## 输入原始代码

调用恰好一次。调用正常返回时返回该值；端口抛出 Timeout 时不重试、不吞掉异常。

```java
class QuoteService {
    private final QuoteClient client;

    QuoteService(QuoteClient client) {
        this.client = client;
    }
    int quote(int id) { return client.fetch(id); }
}
interface QuoteClient { int fetch(int id) throws Timeout; }
class Timeout extends RuntimeException {}
```

## AI 预处理结果

```diff
--- a/src/QuoteService.java
+++ b/src/QuoteService.java
@@ -1,4 +1,7 @@
+import normalizer.annotations.External;
+
 class QuoteService {
+    @External
     private final QuoteClient client;

     QuoteService(QuoteClient client) {
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/QuoteService.java#QuoteService.quote, returns: number }
P: { parameters: { id: { position: 0, type: number } } }
S:
  - call: { external: client.fetch, args: [id], returns: number, result: $e_0 }
R: $e_0
throws:
  - { type: Timeout, when: 'completion.is_throw($e_0, Timeout)' }
```
