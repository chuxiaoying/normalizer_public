# Hello World：布尔代数消去无效开关

## 输入原始代码

入口接收一个分数和一个偏好开关。业务条件要求分数达到 60，偏好开关只出现在两个互补分支中。

```java
class Eligibility {
    static boolean eligible(int score, boolean preferred) {
        boolean active = score >= 60;
        return (active && preferred) || (active && !preferred);
    }
}
```

## AI 预处理结果

**原样通过。** 代码只使用 Java 原生布尔运算和比较，没有外部库、循环或语言扩展。Normalizer 直接接收这段 Java。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留原始 Java，没有本例专用预处理改动。 | 布尔恒等式属于内核表达式化简职责，AI 不预先替内核计算业务答案。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Eligibility.java#Eligibility.eligible
  returns: bool

P:
  parameters:
    score:
      position: 0
      type: number
    preferred:
      position: 1
      type: bool

R: score >= 60
```

`(active && preferred) || (active && !preferred)` 中，`preferred || !preferred` 恒为真，结果只保留 `active`。`preferred` 仍然是完整入口参数，因此保留在 P 中。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old R 保留两个带 `preferred` 的分支，new R 化简为 `score >= 60`。 | 布尔化简应消除对结果没有影响的输入依赖，同时保留完整入口签名。 |
| old 把偏好开关当作业务判断条件，new 只将分数阈值作为结果条件。 | expected 应表达真正决定业务结果的最小条件，不能把源码中的冗余控制流原样带入 PERS。 |
