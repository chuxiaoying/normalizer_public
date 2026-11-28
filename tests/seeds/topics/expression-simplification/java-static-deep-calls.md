# Hello World：深层纯函数调用

## 设计意图

检验多层闭合纯调用等价于同一个组合表达式。插入纯转发层、重命名私有函数应保持结果；改变一层运算或参数绑定应体现到输出，未知实现不能混入。

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

**原样通过。** 三个被调用的方法均在当前类中定义，可以沿原 Java 调用逐层解析。

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
    WHEN limit > base + value + 1 THEN base + value + 1
    ELSE limit
  END
```

三个 helper 均为纯计算。展开调用后，`cap` 的两臂正是已经取得的 `limit` 和 `base + value + 1`；相等时取任一边相同。按 [CF-04](../../../../design/chapters/08-统一定形.md#cf-04-完整二分-case)，简单的 `limit` 放在左侧，两种严格取小候选同成本，选 `>` 并在真臂返回计算值。相等点不因交换两臂而变。
