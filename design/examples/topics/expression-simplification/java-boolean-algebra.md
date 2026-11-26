# Hello World：布尔代数消去无效开关

## 设计意图

对照完全互补的分支与仅看起来相反的两个输入。总谓词的冗余开关可消去；改阈值或把反面换成不同输入后应留下新条件。

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

**预处理要点**：布尔恒等式属于内核表达式化简职责，AI 不预先替内核计算业务答案。

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

另一个验收对照：`(score == 1 || score == 2) && preferred && score != 3` 可以去掉 `score != 3`，因为两个可行值都不是 3，`preferred` 则必须保留。若排除的是 2，就必须失去值 2 的分支，简化成 `preferred AND score == 1`；不能删除这个排除条件后返回原并集。

两端已取得具体整数时，先实际执行比较。`3 <= 3` 和 `-4 < -3` 为真；把前者改成 `3 < 3` 就为假，把等式的右端从 3 改为 4 也为假。这个验收保持入口签名，不能因结果已确定就删掉未用到的参数。

**PERS 验收要点**

- 布尔化简应消除对结果没有影响的输入依赖，同时保留完整入口签名。
- expected 应表达真正决定业务结果的最小条件，不能把源码中的冗余控制流原样带入 PERS。
