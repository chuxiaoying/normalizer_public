# Hello World：业务数值边界比较

## 输入原始代码

入口判断业务数值是否达到零边界。Java 的 `int` 在这里只承载业务 number，不引入宿主位宽语义。

```java
class Bound {
    static boolean safe(int value) {
        return value >= 0;
    }
}
```

## AI 预处理结果

**原样通过。** 这是 Java 原生比较，数值域由统一业务合同解释。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Bound.java#Bound.safe
  returns: bool

P:
  parameters:
    value:
      position: 0
      type: number

R: value >= 0
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都是原生 Java 比较。 | 预处理只标记无法由语言 parser 表达的语义，不把业务 number 改写成 Java 位宽算法。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 可能把 `int` 的机器范围当成业务边界，new 只表达 `value >= 0`。 | PERS 关注业务关系，语言的存储宽度和溢出细节不进入无界业务 number 的内核语义。 |
