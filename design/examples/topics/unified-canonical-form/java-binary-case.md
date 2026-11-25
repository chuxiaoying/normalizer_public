# 完整二分 CASE：正计数保留原值，其余归零

## 输入原始代码

`count` 是已取得的入口整数。零归到默认值，正数保留原值，负数也回到默认值。两臂都是纯值，没有读取、调用或异常。

```java
class CanonicalCount {
    static int clamp(int count) {
        if (count <= 0) return 0;
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** AI 不替内核翻转条件，也不预先计算未知的 `count`。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry:
  function: src/CanonicalCount.java#CanonicalCount.clamp
  returns: number
P:
  parameters:
    count:
      position: 0
      type: number
R: |-
  CASE
    WHEN count > 0 THEN count
    ELSE 0
  END
```

在精确整数域，`count <= 0` 与 `count > 0` 覆盖整域且互斥，所以反转条件同时换两臂不改变结果。按[普通整数计数规则](../../../unified-canonical-form.md#普通整数计数的非正正二分策略)，唯一代表把正值放在 THEN、零放在 ELSE。把左、右操作数对换时也必须同步改比较符号。若阈值变为 `1`，当 `count = 1` 时结果改变，不能与本例合并。
