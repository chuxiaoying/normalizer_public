# Hello World：交换输入后的唯一加法形式

## 设计意图

检验纯全域加法／乘法在排列和不同结合树下得到同一式子。减法和文本拼接是对照，不能因符号相似而排序或消去效果。

## 输入原始代码

入口把两个业务数值相加，参数顺序与结果的唯一表达形式分开观察。

```java
class Order {
    static int total(int base, int value) {
        return value + base;
    }
}
```

## AI 预处理结果

**原样通过。** 加法是 Java 原生表达式，操作数排序交给统一正则规范。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Order.java#Order.total
  returns: number

P:
  parameters:
    base:
      position: 0
      type: number
    value:
      position: 1
      type: number

R: base + value
```

### 同形与异形的判别

- 精确加法：`(value + 2) + base` 和 `2 + (base + value)` 均归到 `base + value + 2`。证明来自同一加法族的交换、结合律；常量处在该规则指定的位置。
- 精确乘法：`value * (base * value)` 和 `(value * value) * base` 均归到 `base * value * value`。同一个因子出现两次仍表示乘两次，不采用 Boolean 的幂等去重。
- 加法和乘法不互相归并；例如 `base = 3, value = 2` 时，两组扩展分别得到 7 和 12。
- 单个完整 CASE 与一个简单、已取得的操作数相减时，可以把减法移进各分支，但不能左右换边。`base - (value > 0 ? 3 : 7)` 与 `(value > 0 ? 3 : 7) - base` 分别保留 `base - 7` 与 `7 - base`。若两边都为 CASE，不套用这项单边规则把两组分支全排列。

### 已取得精确值的同项相减

- 直接入口整数 `value - value` 对每个输入都等于 `0`。两个参数仍留在 P，不因结果不再引用它们而改变入口契约；不产生 E、S 或异常。
- `(value > 0 ? value : value + 2) - value` 只在两条原有首命中域里做减法：正数域为 `0`，零和负数域为 `2`。唯一结果是 `CASE WHEN value <= 0 THEN 2 ELSE 0 END`，不新增观察。这个结论依赖同一个已经取得的精确数值；不允许合并两次求值或把条件取得的东西挪出保护域。

**预处理要点**：交换律和唯一排序属于内核统一正则，预处理不介入可直接解析的表达式。

**PERS 验收要点**：等价表达式必须收敛到唯一形式，expected 才能直接比较而不依赖字符串写法。
