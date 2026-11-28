# Hello World：多出口分支收束为 CASE

## 设计意图

检验多个返回分支汇合成一个 CASE，覆盖低于、处于、超过界限以及边界本身。等价改写不更换结果臂；某个边界值移到另一分支必须改变相应域。

## 输入原始代码

入口对数值执行下界和上界保护，三个返回出口表达同一个分段函数。

```java
class Branch {
    static int clamp(int value, int limit) {
        if (value < 0) return 0;
        if (value > limit) return limit;
        return value;
    }
}
```

## AI 预处理结果

**原样通过。** 原生条件和提前返回已经写清楚，不需要预处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Branch.java#Branch.clamp
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    limit:
      position: 1
      type: number

R: |-
  CASE
    WHEN value < 0 THEN 0
    WHEN limit > value THEN value
    ELSE limit
  END
```

**预处理要点**：已有固定控制流由 normalizer 归一化，AI 只处理工程无法闭合的片段。

**PERS 验收要点**：入口出口可以收束为一个业务函数，同时保留条件优先级和分支顺序。第一臂未命中后，只在 `value >= 0` 的父域内对 `limit` 和 `value` 取小；两种严格候选同成本，按 [CF-04](../../../../design/chapters/08-统一定形.md#cf-04-完整二分-case) 选 `limit > value` 并同步交换两臂，不与第一臂争优先级。没有把 `limit` 默认为非负：若 `value = -1`、`limit = -2`，两个比较都为真，先到达的负值分支返回 0，而不是 −2。
