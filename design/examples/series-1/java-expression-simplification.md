# Hello World：算术抵消、联立约束与布尔化简

## 输入原始代码

入口按两个整数的和与差决定是否返回基础值。原方法接收三个 Java `int` 和一个布尔开关。理想 PERS 用同一业务数值域推导其返回规则。

```java
class Adjustment {
    static int adjust(int base, int x, int y, boolean preferred) {
        long sum = (long) x + y;
        long difference = (long) x - y;
        boolean eligible = sum <= 10 && sum >= 10 && difference == 2;

        if ((eligible && preferred) || (eligible && !preferred)) {
            return (base + x) - x;
        }
        return 0;
    }
}
```

## AI 预处理结果

**原样通过。** 整数运算和分支都由 Java 语法直接表达，上面的代码直接进入 Normalizer。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Adjustment.java#Adjustment.adjust
  returns: number

P:
  parameters:
    base:
      position: 0
      type: number
    x:
      position: 1
      type: number
    y:
      position: 2
      type: number
    preferred:
      position: 3
      type: bool

R: |-
  CASE
    WHEN x == 6 AND y == 4 THEN base
    ELSE 0
  END
```

1. **联立约束。** 和的上下界相同，所以 `x + y = 10`。与 `x − y = 2` 相加得到 `2x = 12`，回代得 `y = 4`。反过来，`(6, 4)` 满足全部约束，是唯一解。
2. **布尔代数。** 从两个分支条件中提出 `eligible`，剩下的 `preferred || !preferred` 恒为真。因此是否命中只取决于 `(x, y)`，开关仍按入口签名列在 P 中。
3. **普通算术。** 命中时，`(base + x) - x` 抵消为 `base`。在业务 number 域中，加减互为逆运算，对任意大小的 base 都成立。未命中时源码直接返回 0。

**PERS 验收要点**

- 条件不能只逐条化简，还应联立求解，直接揭示决定返回值的最小约束。
- 完整调用接口与实际业务依赖各有职责：P 保留入口签名，R、S 与 throws 只引用真正影响结果的输入。
