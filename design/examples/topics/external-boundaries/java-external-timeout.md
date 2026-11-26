# Hello World：外部调用超时

## 设计意图

区分合法端口成功、正常失败与有合同的超时。超时按实际发生域传播，不能凭空合成成功回执；即使缺少后续调用，已经发生的 S 仍保留。

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
@@ -1,10 +1,19 @@
+import normalizer.intrinsics.N;
+
 class QuoteService {
+    @N.External
     private final QuoteClient client;

     QuoteService(QuoteClient client) {
         this.client = client;
     }
     int quote(int id) { return client.fetch(id); }
 }
-interface QuoteClient { int fetch(int id) throws Timeout; }
+class QuoteClient {
+    native int fetch(int id) throws Timeout;
+}
 class Timeout extends RuntimeException {}
+
+class Beans {
+    static final QuoteService ENTRY = new QuoteService(new QuoteClient());
+}
```

分析入口从 `Beans.ENTRY` 取得已装配实例。端口的 `native` 声明只交付原签名，内核按 `@N.External` 记录调用并绑定符号回执，不执行本机代码。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/QuoteService.java#Beans.ENTRY.quote, returns: number }
P: { parameters: { id: { position: 0, type: number } } }
S:
  - call: { external: src/QuoteService.java#Beans.ENTRY.client.fetch, args: [id], returns: number, result: $e_0 }
R: $e_0
throws:
  - { type: Timeout, when: 'completion.is_throw($e_0, Timeout)' }
```
