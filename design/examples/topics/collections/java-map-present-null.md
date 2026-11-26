# Hello World：显式空值覆盖默认标题

## 设计意图

局部 Map 扩展在 `value` 命名空间按完整操作名解析。以 `map_put` 开头的其他标识符不会变成合法写入；普通变量 `value` 也不是 Map 操作。

区分没有该键、该键显式映射到 null 和该键有文本值。不同键、同一键的多次赋值及条件插入应按实际内容分别观察；缺键不读取别的键的空值。空字符串可以是键也可以是已写入的值；它不等于缺键或显式 null。文本键按完整内容区分，大小写不隐式合并。

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

保留方法、if 和 return，并显式声明返回值可空。将局部 HashMap 的构造、put、getOrDefault 在原处绑定为“空映射”“按键覆盖”“按键是否存在取值”。最后一步对存在的键返回原值（含 null），对缺键返回 fallback。这里使用普通可变 HashMap，只有常量字符串键，临时对象不会逃逸。

```diff
--- a/src/Caption.java
+++ b/src/Caption.java
@@ -1,8 +1,13 @@
-import java.util.HashMap;
 class Caption {
+    @normalizer.annotations.Nullable
     static String select(boolean cleared, String fallback) {
-        HashMap<String,String> captions = new HashMap<>();
-        if (cleared) captions.put("main", null);
-        return captions.getOrDefault("main", fallback);
+        java.util.HashMap<String,String> captions;
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
R: |-
  CASE
    WHEN NOT (cleared) THEN fallback
    ELSE null
  END
```

**判别例**　fallback 为“应付”时，cleared 为真返回 null，为假返回“应付”。把查表改成 COALESCE 会在第一种输入上出错。


此完整二分 CASE 按统一正则规范把 null 放在 ELSE，条件同时取反；cleared 为真仍返回 null。当前已验收的是局部、不会逃逸的文本映射。分支汇合先计算键是否存在，再选择该键的实际载荷，不能用载荷是否为 null 推断键不存在。Java 别名、动态键及映射对象身份不在此入口的合同内。

**键的转义对照**　类型化局部 IR 用双引号文本写键；`\\` 保留反斜杠，`\"` 保留双引号。写入和读取同一个键 `"team\\West\"A"` 时，仍区分“键存在且值为 null”和“键不存在”。查找时去掉键中的反斜杠应返回 fallback；`\q` 不是此局部 IR 支持的转义，应拒绝，不能当作字面 q 继续归约。
