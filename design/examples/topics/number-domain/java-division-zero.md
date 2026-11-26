# Hello World：除零保护与真实异常完成

## 设计意图

区分证明除数非零和真实可达的除零错误。已取得的精确整数除自身，只有在该操作确实能正常完成的路径上才可化为一。入口不得依赖 AI 猜测除零规则，也不能用正常值覆盖逃逸异常。

## 输入原始代码

```java
class Ratio {
    static int safeDivide(int divisor) {
        if (divisor == 0) return 0;
        return divisor / divisor;
    }

    static int unchecked(int divisor) {
        return divisor / divisor;
    }
}
```

## AI 预处理结果

**原样通过。** 整数除法和保护条件都是 Java 原生语义。源码除法失败时抛出 `java.lang.ArithmeticException`，不由预处理移除除法或补默认值；内核先取得操作数，再应用非零域的化简规则。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Ratio.java#Ratio.safeDivide, returns: number }
P:
  parameters:
    divisor: { position: 0, type: number }
R: CASE WHEN divisor == 0 THEN 0 ELSE 1 END
```

0 返回 0；正负非零整数都返回 1。两个出口都是正常完成，且没有可观察动作。去掉保护时，0 路径真实抛出，不再返回：

```yaml
schema: normalizer.pers/1
entry: { function: src/Ratio.java#Ratio.unchecked, returns: number }
P:
  parameters:
    divisor: { position: 0, type: number }
R: '1'
throws:
  - { type: java.lang.ArithmeticException, when: divisor == 0 }
```

这里的 R 只描述正常完成域。两端是不同整数时，不能套用同值相除规则；整数商仍按趋零规则计算，参见[具体商与余数规则](../unified-canonical-form/java-constant-quotient.md)。

## 可准入的同形对照

这不是用加法替换除法的预处理。它是独立入口，用原生加法合同核对相同的参数、零值守卫与早返回形态本身不改变准入。

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
