# Java：普通标量 CASE 什么时候要提取 LET

## 设计意图

先化简整个业务值，再决定是否给仍然重复的子式命名。普通标量和含 CASE 的聚合用同一原则：两处引用必须在同一取得域，且绑定后严格减少结构。源码局部名和分支写法不决定输出。

## 输入原始代码

```java
class CaseParity {
    static int evaluate(boolean enabled, int a, int b, int shift, boolean include) {
        int selected = enabled ? a : b;
        return (selected + shift) * (selected - shift);
    }
}
```

## AI 预处理结果

**原样通过。** 条件表达式及精确整数运算都是 Java 原生写法。不能让预处理替内核分配 LET，也不需要把原程序改成 CASE。

## 归一化最终结果

`selected` 的两个引用都已取得。单独把任一加减法移进 CASE 只会复制运算，不减少条件臂；乘法的直接操作数并不是相同的 CASE。不做无收益的展开，保留重复子式，提取一次就少写一份完整 CASE。减法的操作数次序不变。

```yaml
schema: normalizer.pers/1
entry: { function: src/CaseParity.java#CaseParity.evaluate, returns: number }
P:
  parameters:
    enabled: { position: 0, type: bool }
    a: { position: 1, type: number }
    b: { position: 2, type: number }
    shift: { position: 3, type: number }
    include: { position: 4, type: bool }
R: |-
  LET $l_0 = CASE WHEN enabled THEN a ELSE b END
  IN (shift + $l_0) * ($l_0 - shift)
```

| 对照 | 必须保留的区别 |
| --- | --- |
| 反写条件并同时交换两臂，改局部名 | PERS 与原例完全一致。 |
| 只返回 `selected - shift` | 只使用一次，保持内联。 |
| 直接返回 `selected * selected` | 两个相同的完整 CASE 可同步取臂；先化为 `CASE WHEN enabled THEN a * a ELSE b * b END`，没有需要提取的重复。 |
| 原表达式再加 `include ? selected + shift : 0` | 第三个引用仍在外层条件域内内联；前两个同域引用仍共享一个 LET，不把这第三处纳入共同绑定。 |
