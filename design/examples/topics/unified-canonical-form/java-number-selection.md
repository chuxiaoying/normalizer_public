# 取小值：同一总序下，四种比较写法归到同一个 CASE

## 设计意图

检验同一总序中的取小／取大对换条件与分支后保持唯一代表，并覆盖小于、等于、大于。把取小改取大或改变并列语义不属于等价。

## 输入原始代码

额度和账单金额都是已取得的入口整数。只退其中较小的值；当两值相等时，取哪一边不改变返回值。

```java
class CanonicalQuota {
    static int smaller(int quota, int total) {
        if (total <= quota) return total;
        return quota;
    }
}
```

## AI 预处理结果

**原样通过。** 不改判断条件或分支次序，不提前选择哪边更小。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalQuota.java#CanonicalQuota.smaller, returns: number }
P:
  parameters:
    quota: { position: 0, type: number }
    total: { position: 1, type: number }
R: |-
  CASE
    WHEN quota < total THEN quota
    ELSE total
  END
```

两个候选都就是比较的原值，且合同保证全序。相等处值不可区分，因此统一用严格比较；两个表达式同大小，结构键选 `quota`。写成 `total < quota` 取 `total`、`quota <= total` 取 `quota` 或规范写法，都是取小值。刻意改为取大值则必须归到 `quota > total`，如 `quota=3, total=10` 时两种业务分别返回 3 和 10。此规则不推及 NaN、未知类型、不同返回臂或会重新执行的调用。
