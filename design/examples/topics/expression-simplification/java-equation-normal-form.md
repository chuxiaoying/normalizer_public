# Hello World：等式比较的唯一方向

## 输入原始代码

入口判断输入加一是否等于目标值。等式两侧的规范方向由内核统一选择。

```java
class Equation {
    static boolean matches(int value, int target) {
        return value + 1 == target;
    }
}
```

## AI 预处理结果

**原样通过。** Java 等式和加法属于当前 parser 可处理的纯表达式。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Equation.java#Equation.matches
  returns: bool

P:
  parameters:
    value:
      position: 0
      type: number
    target:
      position: 1
      type: number

R: target == value + 1
```

**预处理要点**：可计算的等式化简交给内核，预处理不代替数学推导。

**PERS 验收要点**：等式方向也纳入统一正则，等价断言不依赖源代码左右顺序。
