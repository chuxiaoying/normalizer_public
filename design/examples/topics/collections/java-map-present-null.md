# Hello World：显式空值覆盖默认标题

[本主题](README.md)

## 输入原始代码

临时标题表只在调用中使用；fallback 非空，用户可以把已有标题明确清空。

```java
import java.util.HashMap;
class Caption {
    static String select(boolean cleared, String fallback) {
        HashMap<String,String> captions = new HashMap<>();
        if (cleared) captions.put("main", null);
        return captions.getOrDefault("main", fallback);
    }
}
```

## AI 预处理结果

保留方法、if 和 return。将局部 HashMap 的构造、put、getOrDefault 在原处绑定为“空映射”“按键覆盖”“按键是否存在取值”。最后一步对存在的键返回原值（含 null），对缺键返回 fallback。这里使用普通可变 HashMap，只有常量字符串键，临时对象不会逃逸。

```diff
--- a/src/Caption.java
+++ b/src/Caption.java
@@ -1,8 +1,13 @@
 import java.util.HashMap;
 class Caption {
     static String select(boolean cleared, String fallback) {
-        HashMap<String,String> captions = new HashMap<>();
-        if (cleared) captions.put("main", null);
-        return captions.getOrDefault("main", fallback);
+        HashMap<String,String> captions;
+        /* @normalizer-ir captions = value.map_empty(text, text?) */
+        if (cleared) {
+            /* @normalizer-ir captions = value.map_put(captions, "main", null) */
+        }
+        String selected;
+        /* @normalizer-ir selected = value.map_get_or(captions, "main", fallback) */
+        return selected;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Caption.java#Caption.select
  returns: text?
P:
  parameters:
    cleared: { position: 0, type: bool }
    fallback: { position: 1, type: text }
R: CASE WHEN cleared THEN null ELSE fallback END
```

**判别例**　fallback 为“应付”时，cleared 为真返回 null，为假返回“应付”。把查表改成 COALESCE 会在第一种输入上出错。
