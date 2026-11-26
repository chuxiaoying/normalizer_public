# Hello World：小数舍入与显示边界

## 设计意图

检验正负中点、显式舍入模式和二进制浮点无法精确表示的值。没要求保留两位时不自行舍入；模式、位数或中点两侧变化应按源码产生不同数值。

## 输入原始代码

本例保留把金额舍入到两位小数的计算。源码直接嵌入局部 IR `round(value, 2, HALF_UP)`，用 `java.math.BigDecimal` 标记精确十进制数值，不把原生 `double` 的二进制近似当成精确小数。没有调用 Java 的舍入库，也没有把金额格式化成文本。这里的 2 和 HALF_UP 只属于这个例子，不是金额类型的默认位数或中点模式。显式指定其他位数或模式时保留它们，未要求舍入时不插入 ROUND。

```java
class DecimalPrice {
    static java.math.BigDecimal roundPrice(java.math.BigDecimal value) {
        java.math.BigDecimal rounded;
        /* @normalizer-ir
           rounded = round(value, 2, HALF_UP)
        */
        return rounded;
    }
}
```

## AI 预处理结果

**原样通过。** 场景规范源码已用局部 `round` 语义表达舍入。AI 不把小数转成 Java 固定宽度整数，也不预先计算具体金额。

**预处理要点**：小数精度和显示边界必须有独立数值合同，不能把宿主库函数名当成业务规则，也不能把一次样例结果写成常量。

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
