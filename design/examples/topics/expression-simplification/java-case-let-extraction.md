# Hello World：只有重复的 CASE 表达式允许提取 LET

## 设计意图

对照重复 CASE、仅出现一次、重复但不含 CASE 和条件才取得的值。只有含 CASE 且确实重复才允许 LET；即使可提取也不能跨不合法取得域。

## 输入原始代码

`adjusted` 是纯算术 `value + 1`，在外层 CASE 的条件和 ELSE 中重复使用。但被重复的表达式本身不含 CASE WHEN，不能提取 LET。派生用例对照仅用一次的 CASE。

```java
class CaseReuse {
    static int cap(int value, int limit, int base) {
        int adjusted = value + 1;
        return adjusted > limit ? limit : adjusted + base;
    }
}
```

## AI 预处理结果

**原样通过。** 这是合法的原生 Java。AI 不提前展开 `adjusted`，也不把 CASE 改写成共享扩展语法。局部重复的提取属于统一 PERS 规范化职责。

**预处理要点**：是否允许 LET 由统一 PERS 规则决定，不由宿主语言预处理决定。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/CaseReuse.java#CaseReuse.cap
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    limit:
      position: 1
      type: number
    base:
      position: 2
      type: number

R: |-
  CASE
    WHEN limit < value + 1 THEN limit
    ELSE base + value + 1
  END
```

**PERS 验收要点**：外层是 CASE 不等于被绑定值含 CASE。只有同一纯值本身含 CASE WHEN 且实际有两处或更多引用时，PERS 才允许提取 LET；不得改变观察、异常或惰性。

## 被重复的聚合本身包含完整 CASE

```java
class CaseReuse {
    static int square(int[] values, boolean enabled, int extra) {
        int offset = enabled ? extra : 0;
        int total;
        /* @normalizer-ir
           total = sum from values as value yield value + offset
        */
        return total * total;
    }
}
```

**原样通过。** 相同聚合是乘法的两个已取得操作数，查询只依赖入口数组、行绑定和已取得的标量参数。CASE 在被绑定的表达式里面而不在外层保护域，因此可以在 R 的作用域提取一次。若只返回一次总和，或者偏移改为不含 CASE 的普通入参，则不能提取。

```yaml
schema: normalizer.pers/1
entry: { function: src/CaseReuse.java#CaseReuse.square, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    enabled: { position: 1, type: bool }
    extra: { position: 2, type: number }
R: |-
  LET $l_0 = COALESCE(SUM(SELECT value + CASE WHEN enabled THEN extra ELSE 0 END FROM values), 0)
  IN $l_0 * $l_0
```

若两次引用只出现在外层返回 guard 的某一臂，不能为提取 LET 把计算移到 guard 之外。
