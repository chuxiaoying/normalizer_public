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

**原样通过。** 加法是 Java 原生表达式，操作数排序交给统一归一化规范。

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

**预处理要点**：交换律和唯一排序属于内核统一正则，预处理不介入可直接解析的表达式。

**PERS 验收要点**：等价表达式必须收敛到唯一形式，expected 才能直接比较而不依赖字符串写法。
