# Hello World：无法证明终止的递归必须拒绝

## 设计意图

区分有真实下降度量的递归、有基本情况却不能证明下降，以及明显循环调用。未证明终止时不能用固定深度的结果代替业务语义。

## 输入原始代码

入口递归调用自身。当 `value > 0` 时，下一次递归仍传入同一个值；有基本情况不等于每条路径都能到达基本情况。

```java
class Recursive {
    static int down(int value) {
        if (value <= 0) return 0;
        return down(value) + 1;
    }
}
```

## AI 预处理结果

**拒绝。** AI 不截断递归深度，不把递归调用改成固定次数循环，也不凭借参数名猜测终止合同。应报告正数路径的调用环；不能编造有界结束行为。

诊断指出 `down(value)` 在正数路径上的回边没有下降度量；不能用一份部分结果代替完整求值。

**预处理要点**：符号化执行不能在不知道递归深度时假装得到完整结果。无法证明终止时，拒绝比截断或补默认值可靠。

## 归一化最终结果

未生成 PERS。如果仍交给分析器，诊断应指出 `Recursive.down` 无法完成完整分析；不把预算或终止证明的缺口当业务异常。

**PERS 验收要点**：递归终止合同属于进入内核前的准入条件，不能把不完整的递归执行包装成一个看似完整的 R。

双函数对照保留 `down` 的基本情况，但当 `value > 0` 时，`down(value)` 调用 `step(value)`，后者再以同一实参调用 `down(value)`。状态没有下降，两条调用边组成同一个环。所选入口同样必须被明确拒绝且无 PERS；不能只检查函数是否直接调用自己。

## 有下降证明的独立对照

```java
class Recursive {
    static int finiteDown(int value) {
        if (value <= 0) return 0;
        return finiteDown(value - 1) + 1;
    }
}
```

**原样通过。** 这里每条进入递归的路径都有 `value > 0`，非负整数下降度量每次减 1；调用链有限，且每次返回再加 1。按业务 number 表示，不把宿主栈容量变成业务阈值；实际分析资源不够应报告未完成，不能输出截断结果。

```yaml
schema: normalizer.pers/1
entry: { function: src/Recursive.java#Recursive.finiteDown, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: |-
  CASE
    WHEN value > 0 THEN value
    ELSE 0
  END
```
