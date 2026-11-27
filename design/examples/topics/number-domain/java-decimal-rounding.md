# Hello World：小数舍入与显示边界

## 设计意图

检验正负中点、显式舍入模式和二进制浮点无法精确表示的值。没要求保留两位时不自行舍入；模式、位数或中点两侧变化应按源码产生不同数值。

准入检查只核对值、位数和模式的类型及签名，不提前执行舍入。保留已经声明的位数或模式差异，不能暗用宿主的默认舍入。

## 输入原始代码

本例保留把金额舍入到两位小数的计算。生产源码用 `BigDecimal.setScale(2, RoundingMode.HALF_UP)` 表达金额舍入，用 `BigDecimal` 承载精确十进制数值；不把二进制浮点近似当成精确小数，也不把金额格式化成文本。这里的 2 和 HALF_UP 只属于这个例子，不是金额类型的默认位数或中点模式。显式指定其他位数或模式时保留它们，未要求舍入时不插入 ROUND。

```java
import java.math.BigDecimal;
import java.math.RoundingMode;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        return value.setScale(2, RoundingMode.HALF_UP);
    }
}
```

## AI 预处理结果

保留精确数值载体，换成第四章定义的标准运算；不另造注释内的通用舍入指令，不把小数转成固定宽度整数，也不预先计算金额。

```diff
--- a/src/DecimalPrice.java
+++ b/src/DecimalPrice.java
@@ -1,8 +1,8 @@
 import java.math.BigDecimal;
-import java.math.RoundingMode;
+import normalizer.intrinsics.N;

 class DecimalPrice {
     static BigDecimal roundPrice(BigDecimal value) {
-        return value.setScale(2, RoundingMode.HALF_UP);
+        return N.round(value, 2, N.HALF_UP);
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/DecimalPrice.java#DecimalPrice.roundPrice
  returns: number

P:
  parameters:
    value: { position: 0, type: number }

R: 'ROUND(value, 2, "HALF_UP")'
```

在明确指定 HALF_UP 的用例里，2.345 按两位得到 2.35，-2.345 得 -2.35；明确指定 HALF_EVEN 时两者为 2.34 和 -2.34。没有提供模式的两参数写法不推断中点方向。

**PERS 验收要点**：数值域的精度、舍入方向、负数边界和显示位数应由统一数值合同定义，不能隐含在 Java 或第三方库实现里。

精确小数须区分中点两侧。HALF_EVEN 根据保留最后一位的奇偶决定方向，2.355 保留两位为 2.36，2.345 为 2.34。


若乘数和被乘数都是精确小数，先按精确数值计算乘积。以 4 位 HALF_EVEN 对 `N.multiply` 得到的 1.25 × 2.5 舍入时，乘积是 `3.125`，请求的四位大于实际的三位小数，数值不变。规范结果只写 `3.125`，不保留 ROUND 或补末尾零。

对未知输入保留 `ROUND` 的数值、舍入位数和舍入模式。普通乘法没有这三个合同要素，不能改写成舍入。
