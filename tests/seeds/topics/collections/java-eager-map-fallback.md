# 缺省值没被采用，调用仍已发生

## 设计意图

按[关系指令的查键规则](../../../../design/chapters/05-关系代数.md#其他局部扩展)，选择映射结果和求值后备实参是两件事。原 Java 调用先求值实参，即使键存在且对应值是 null，后备端口也已调用；端口失败会阻止正常返回。

## 输入原始代码

观察从已装配实例的入口开始。标题服务可能正常返回非空文本，也可能抛出声明的异常；两种情况下都不能用表里已有的空值跳过调用。

```java
import java.util.HashMap;

class CaptionUnavailable extends RuntimeException {}
interface CaptionService {
    String get() throws CaptionUnavailable;
}
class FallbackCaption {
    private final CaptionService defaults;
    FallbackCaption(CaptionService defaults) { this.defaults = defaults; }

    String select(boolean cleared) {
        HashMap<String, String> captions = new HashMap<>();
        if (cleared) captions.put("main", null);
        return captions.getOrDefault("main", defaults.get());
    }
}
```

## AI 预处理结果

用原处的关系指令表达局部映射。把后备调用的结果先存到局部变量，再查键，不把该调用搬进缺键分支。外部标记只声明真实端口；静态根确定已经装配的接收者。

```diff
--- a/src/FallbackCaption.java
+++ b/src/FallbackCaption.java
@@ -1,16 +1,25 @@
-import java.util.HashMap;
+import normalizer.intrinsics.N;

+class CaptionMap {}
 class CaptionUnavailable extends RuntimeException {}
-interface CaptionService {
-    String get() throws CaptionUnavailable;
+class CaptionService {
+    native String get() throws CaptionUnavailable;
 }
 class FallbackCaption {
-    private final CaptionService defaults;
+    static final FallbackCaption INSTANCE = new FallbackCaption(new CaptionService());
+
+    @N.External private final CaptionService defaults;
     FallbackCaption(CaptionService defaults) { this.defaults = defaults; }

-    String select(boolean cleared) {
-        HashMap<String, String> captions = new HashMap<>();
-        if (cleared) captions.put("main", null);
-        return captions.getOrDefault("main", defaults.get());
+    @N.Nullable String select(boolean cleared) {
+        CaptionMap captions;
+        /* @normalizer-ir captions = value.map_empty(text, text?) */
+        if (cleared) {
+            /* @normalizer-ir captions = value.map_put(captions, "main", null) */
+        }
+        String fallback = defaults.get();
+        @N.Nullable String selected;
+        /* @normalizer-ir selected = value.map_get_or(captions, "main", fallback) */
+        return selected;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/FallbackCaption.java#FallbackCaption.INSTANCE.select, returns: 'text?' }
P:
  parameters:
    cleared: { position: 0, type: bool }
S:
  - call:
      args: []
      external: src/FallbackCaption.java#FallbackCaption.INSTANCE.defaults.get
      result: $e_0
      returns: text
throws:
  - type: CaptionUnavailable
    when: completion.is_throw($e_0, CaptionUnavailable)
R: CASE WHEN cleared THEN null ELSE $e_0 END
```

端口成功、键存在时 R 为 null；缺键才返回这次响应。端口失败时，S 仍有一次调用，出口为 CaptionUnavailable。始终插入 null 或始终不插入的两个对照，分别让正常结果只剩 null 或回执；两者保留原 P、同一个 S 和异常合同。
