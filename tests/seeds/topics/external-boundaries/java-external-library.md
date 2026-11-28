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

预处理替换导入与两次调用的目标，并在允许空值的 raw 参数上保留输入合同。fallback 按普通文本的非空默认规则处理。入口从 `Beans.ENTRY` 定位已装配的接收者。

```diff
--- a/src/LabelService.java
+++ b/src/LabelService.java
@@ -1,10 +1,14 @@
-import org.apache.commons.lang3.StringUtils;
+import normalizer.intrinsics.N;

 class LabelService {
-    String normalizeLabel(String raw, String fallback) {
-        return StringUtils.defaultIfBlank(
-            StringUtils.trim(raw),
+    String normalizeLabel(@N.Nullable String raw, String fallback) {
+        return N.defaultIfBlank(
+            N.trim(raw),
             fallback
         );
     }
 }
+
+class Beans {
+    static final LabelService ENTRY = new LabelService();
+}
```

本例的两处调用绑定到以下文本操作。

| 声明上的静态方法 | 共享语义 |
|---|---|
| `String trim(String value)` | `N.trim(value)`。输入为 null 时返回 null，否则仅去掉两端值不超过 U+0020 的 UTF-16 码元。 |
| `String defaultIfBlank(String value, String fallback)` | `N.defaultIfBlank(value, fallback)`。null、空串和所有码元均满足 Java 17 `Character.isWhitespace(char)` 的字符串取 fallback。 |

内层 `trim(raw)` 仍只调用一次。U+2003 经 trim 后仍是空白，结果为 fallback；U+00A0 则原样返回。

具体 `null` 先用同一份函数合同执行，`trim(null)` 得到 null，空白判断遇到 null 则为真。符号化的空白条件也使用这份合同；不在文本适配器和化简流水线分别复制这两条结果。

**预处理要点**：在调用位置交付定义完整的公共文本语义，使预处理无需另写整段字符算法或改变业务接口载体；内核也无需按库名特判。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/LabelService.java#Beans.ENTRY.normalizeLabel
  returns: text

P:
  parameters:
    raw:
      position: 0
      type: text?
    fallback:
      position: 1
      type: text

R: N.defaultIfBlank(N.trim(raw), fallback)
```

本例的两个文本操作都是纯计算，因此结果只有一个返回表达式。 变体将 `raw` 换成原源码中的具体文本，先验证内核对普通文本、空白和非断行空格的执行结果；原例继续核对未知文本的符号公式。

**PERS 验收要点**：最终表达应呈现文本操作的业务含义，字符遍历与编码结构留在操作合同内；公共文本语义仍须覆盖完整原始字符域，不能用更简短的公式掩盖孤立代理项。

**空值守卫对照**　将内层换为原始 `raw`，再显式写成 `raw == null ? fallback : N.defaultIfBlank(raw, fallback)`。[标准操作的合同](../../../../design/chapters/04-AI救赎.md#ai-02-标准操作的合同)已经覆盖 null，因此 R 是 `N.defaultIfBlank(raw, fallback)`。非空但属于空白的文本仍返回 fallback；非空白文本仍保留原值。无需额外识别旧 Java 库实现里的空值写法。

直接给纯文本内建操作传 `null` 与经 `trim(null)` 取得的空值同属文本语义：第一实参为空时选备用文本。后备值按本入口约定非空，不用一个永远不选中的值绕过类型检查。两种情况都不生成 S。
