# Hello World：深层纯函数调用

## 输入原始代码

这个案例使用多层纯 helper 调用，但不增加状态和 external 边界。归一化需要穿透纯函数调用，把最终业务关系表达出来。

```java
class Pricing {
    static int normalize(int value) {
        return value + 1;
    }

    static int addBase(int value, int base) {
        return value + base;
    }

    static int cap(int value, int limit) {
        return value > limit ? limit : value;
    }

    static int calculate(int value, int base, int limit) {
        int normalized = normalize(value);
        int withBase = addBase(normalized, base);
        return cap(withBase, limit);
    }
}
```

## AI 预处理结果

**原样通过。** 三个被调用的方法均在当前类中定义，可以沿原 Java 调用逐层解析。上面的 Java 即为 Normalizer 的输入。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留三个 helper 和原入口，不改写 Java；无本例预处理差异。 | — |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Pricing.java#Pricing.calculate
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    base:
      position: 1
      type: number
    limit:
      position: 2
      type: number

R: |-
  CASE
    WHEN limit < base + value + 1 THEN limit
    ELSE base + value + 1
  END
```

三个 helper 均为纯计算，最终的额度判断只读取 `value`、`base` 和 `limit`。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| 无本例独有差异。old 已能穿透三个 helper；共性变化见[基础案例](java-static.md#归一化最终结果)。 | — |
