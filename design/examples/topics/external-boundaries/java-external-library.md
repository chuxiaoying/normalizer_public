# Hello World：Apache Util 调用转换为纯表达式

## 设计意图

对比有明确纯语义合同的库调用与普通无源码调用。空文本、仅空白和非空文本应由已登记谓词区分；只改写拼写不应产生 S，也不能把未知库方法猜成同一谓词。

## 输入原始代码

`normalizeLabel` 先去掉两端字符，再为空白结果选择备用文本。两处调用均来自 Apache Commons。按[统一业务类型合同](../../../../implementation/contracts/pers/schema.yaml)，本例原始文本 `raw` 显式允许 null，备用文本 `fallback` 非空；空字符串仍是合法文本，不能与 null 混同。

```java
import org.apache.commons.lang3.StringUtils;

class LabelService {
    String normalizeLabel(String raw, String fallback) {
        return StringUtils.defaultIfBlank(
            StringUtils.trim(raw),
            fallback
        );
    }
}
```

## AI 预处理结果

预处理替换导入与两次调用的目标，并在允许空值的 raw 参数上保留输入合同。fallback 按普通文本的非空默认规则处理。

```diff
--- a/src/LabelService.java
+++ b/src/LabelService.java
@@ -1,9 +1,9 @@
-import org.apache.commons.lang3.StringUtils;
+import normalizer.intrinsics.NormalizedText;

 class LabelService {
-    String normalizeLabel(String raw, String fallback) {
+    String normalizeLabel(@normalizer.annotations.Nullable String raw, String fallback) {
-        return StringUtils.defaultIfBlank(
-            StringUtils.trim(raw),
+        return NormalizedText.defaultIfBlank(
+            NormalizedText.trim(raw),
             fallback
         );
     }
```

本例的两处调用绑定到以下文本操作。

| 声明上的静态方法 | 共享语义 |
|---|---|
| `String trim(String value)` | `text.trim(value)`。输入为 null 时返回 null，否则仅去掉两端值不超过 U+0020 的 UTF-16 码元。 |
| `String defaultIfBlank(String value, String fallback)` | `text.is_blank(value) ? fallback : value`。null、空串和所有码元均满足 Java 17 `Character.isWhitespace(char)` 的字符串取 fallback。 |

内层 `trim(raw)` 仍只调用一次。U+2003 经 trim 后仍是空白，结果为 fallback；U+00A0 则原样返回。

具体 `null` 先用同一份函数合同执行，`trim(null)` 得到 null，空白判断遇到 null 则为真。符号化的空白条件也使用这份合同；不在文本适配器和化简流水线分别复制这两条结果。

**预处理要点**：在调用位置交付定义完整的公共文本语义，使预处理无需另写整段字符算法或改变业务接口载体；内核也无需按库名特判。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/LabelService.java#LabelService.normalizeLabel
  returns: text

P:
  parameters:
    raw:
      position: 0
      type: text?
    fallback:
      position: 1
      type: text

R: |-
  CASE
    WHEN blank(TRIM(raw)) THEN fallback
    ELSE TRIM(raw)
  END
```

本例的两个文本操作都是纯计算，因此结果只有一个返回表达式。 变体将 `raw` 换成原源码中的具体文本，先验证内核对普通文本、空白和非断行空格的执行结果；原例继续核对未知文本的符号公式。

**PERS 验收要点**：最终表达应呈现文本操作的业务含义，字符遍历与编码结构留在操作合同内；公共文本语义仍须覆盖完整原始字符域，不能用更简短的公式掩盖孤立代理项。

**空值守卫对照**　将内层换为原始 `raw`，再显式写成 `raw == null ? fallback : NormalizedText.defaultIfBlank(raw, fallback)`。注册的空白谓词自身已经覆盖 null，因此 R 是 `CASE WHEN blank(raw) THEN fallback ELSE raw END`。非空但属于空白的文本仍返回 fallback；非空白文本仍保留原值。无需额外识别旧 Java 库实现里的空值写法。
