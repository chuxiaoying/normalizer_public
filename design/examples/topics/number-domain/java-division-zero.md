# Hello World：除零合同未闭合时拒绝归一化

## 输入原始代码

源码显式保护了除数为零的路径，但正常路径仍使用当前 scalar-0 尚未闭合的除法表达式。

```java
class Ratio {
    static int safeDivide(int total, int divisor) {
        if (divisor == 0) return 0;
        return total / divisor;
    }
}
```

## AI 预处理结果

**拒绝。** AI 不能把 `total / divisor` 偷换成乘倒数、固定结果或外部黑盒。要继续归一化，必须先提供除法及其除零行为的明确业务合同。

```text
PREPROCESS_REQUIRED

safeDivide:
  expression: total / divisor
  required: an explicit division and zero-divisor contract
```

## 归一化最终结果

未生成 PERS。CLI 应以非零状态返回 `PREPROCESS_REQUIRED`，stdout 为空。

**预处理要点**：预处理只能补足可证明的语义，不能用数学猜测掩盖内核尚未支持的操作。

**PERS 验收要点**：除零行为属于业务合同，未定义时拒绝比输出一个看似合理的默认值更可靠。

## 可准入的同形对照

这不是用加法替换除法的预处理。它是独立入口，用已有加法合同核对相同的参数、零值守卫与早返回形态本身不应触发拒绝。

```java
class AdditionControl {
    static int addNonzero(int total, int operand) {
        if (operand == 0) return 0;
        return total + operand;
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/AdditionControl.java#AdditionControl.addNonzero, returns: number }
P:
  parameters:
    total: { position: 0, type: number }
    operand: { position: 1, type: number }
R: |-
  CASE
    WHEN operand == 0 THEN 0
    ELSE operand + total
  END
```
