# Hello World：金额按最小单位保持精度

## 设计意图

检验金额按最小单位的精确计算。调整主单位或次单位改变结果，超出浮点精度也不得回卷或舍入；显示位数不是数值复制的额外规则。

## 输入原始代码

业务把主单位和最小单位合成为整数金额。计算过程没有浮点舍入，精度合同由业务表达式本身给出。

```java
class Money {
    static int toMinorUnits(int major, int minor) {
        return major * 100 + minor;
    }
}
```

## AI 预处理结果

**原样通过。** 金额的缩放因子和加法已经在业务源码中明确写出，AI 不替工程提前求值，也不引入 `money` 库特判。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Money.java#Money.toMinorUnits
  returns: number

P:
  parameters:
    major:
      position: 0
      type: number
    minor:
      position: 1
      type: number

R: major * 100 + minor
```

**预处理要点**：精度规则已经是业务代码时，AI 只交付原生表达式，不能用金额库名替代规则。

**PERS 验收要点**：PERS 只保留业务数值关系，宿主位宽和机器溢出不在源码明确表达时不进入结果。
