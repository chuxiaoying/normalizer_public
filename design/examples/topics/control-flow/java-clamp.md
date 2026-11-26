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

**原样通过。** Java 的条件和返回语句属于当前 parser 能直接处理的控制流，AI 不重写入口。

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
    WHEN limit < value THEN limit
    ELSE value
  END
```

**预处理要点**：已有固定控制流由 normalizer 归一化，AI 只处理工程无法闭合的片段。

**PERS 验收要点**：入口出口可以收束为一个业务函数，同时保留条件优先级和分支顺序。
