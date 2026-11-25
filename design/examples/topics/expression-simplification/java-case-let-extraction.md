# Hello World：只有重复的 CASE 表达式允许提取 LET

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

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留局部声明、CASE 条件和返回表达式。 | 是否允许 LET 由统一 PERS 规则决定，不由宿主语言预处理决定。 |

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

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 通过局部变量引用 `value + 1`；new 在 CASE 条件和 ELSE 中直接展开它。 | 外层是 CASE 不等于被绑定值含 CASE。只有同一纯值本身含 CASE WHEN 且实际有两处或更多引用时，PERS 才允许提取 LET；不得改变观察、异常或惰性。 |
