# 有条件值的两臂必须有相容类型

## 设计意图

在真假两侧都可达的入口中，分别改变候选返回值的类型和所在臂。相同业务标量的合法选择保留条件；不同标量、序列与标量不能因交换臂而变得合法。无法闭合的类型组合要报告实际冲突，不应在构造诊断时触发内核内部异常。

## 输入原始代码

```java
class Shape {
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
```

## AI 预处理结果

**原样通过。** `choose` 是普通布尔入参，不在预处理阶段猜哪一臂发生。数组参数是入口输入，本例不读取其内容。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Shape.java#Shape.read, returns: number }
P:
  parameters:
    choose: { position: 0, type: bool }
    values: { position: 1, type: 'number[]' }
    value: { position: 2, type: number }
R: CASE WHEN choose THEN value ELSE 1 END
```

| 变化 | 独立验收 |
|---|---|
| 两臂仍是数值，改变默认值 | 保留真实条件，只有默认值变化 |
| 数值与布尔值进入两臂 | 两侧都可达，不猜转换；类型诊断、无 PERS |
| 任一臂选中整条数组，另一臂选中数值 | 保留 then/else 的实际类型与方向；类型诊断、无 PERS，不能内部 Panic |

这是有限的类型准入对照，不宣称取代 Java 编译器的完整静态类型检查。非法无源码调用的 [专用 Panic 合同](java-unsupported-source-boundary.md) 不受影响。
