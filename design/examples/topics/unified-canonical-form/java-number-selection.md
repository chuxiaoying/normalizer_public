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


同一选择也适用于解析器明确交付为有限、精确业务数值的直接入口参数。派生验收用 Java 的 `double` 声明进入这一业务值域：`quota < total ? quota : total`、`quota <= total ? quota : total` 和 `total <= quota ? total : quota` 归到上面同一个严格 CASE。例如输入 `quota=1.25, total=2.5` 返回 `1.25`；交换两值仍返回 `1.25`；两者均为 `1.25` 时条件严格与否不改变返回。它不声明 JVM 二进制浮点的 NaN、无穷大或有符号零语义，也不把未取得的字段或回执当成全域可读。

### 取值范围与结构排序

普通文本不是数值。即使字段是数值，也要先证明本次合法取得，才可以比较两个取小值写法是否等价。

结构排序遵循[第八章的排序键](../../../chapters/第八章%20·%20统一定形规范.md#第三步同一套排序键p-和-e-的名字最后裁决)。回执字段路径与裸回执标量的来源种类不同，不能相互冒充。先比较类型、运算及结构，公开路径仅在前几级都打平时参与裁决。标量参数也不会被纯调用中更早出现的内部空集合载体冒充。只给已取得的表达式定形，不调整 S 的次序。
