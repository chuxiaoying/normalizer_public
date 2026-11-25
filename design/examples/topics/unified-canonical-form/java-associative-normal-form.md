# Hello World：交换输入后的唯一加法形式

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

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保持 Java 加法。 | 交换律和唯一排序属于内核的统一正则规范，预处理不介入可直接解析的表达式。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 的源码写法为 `value + base`，new 固定为 `base + value`。 | 等价表达式必须收敛到唯一形式，expected 才能直接比较而不依赖字符串写法。 |
