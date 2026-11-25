# Java 常量数组：按源码位置执行，不按元素值排序

## 输入原始代码

数组里的两个数是按位置记录的十位和个位，不能交换。带长度的分配与带一个初始元素的写法不是同一种分配。

```java
class CanonicalArray {
    static int digits() {
        int[] values = {2, 7};
        return values[0] * 10 + values[1];
    }
}
```

## AI 预处理结果

**原样通过。** AI 保留原生初始化、数组长度与读取位置，不提前算出常量。显式写成 `new int[]{2, 7}`，或者在最后一个元素后保留合法的尾逗号，归一化合同相同。带括号的整数比较作为元素表达式的一部分时，逗号仍分隔数组元素。 `int[] values` 与 `int values[]` 给同一个声明类型；在 `int first, values[]` 里，后置括号只属于 `values`（[JLS 10.2](https://docs.oracle.com/javase/specs/jls/se21/html/jls-10.html#jls-10.2)）。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalArray.java#CanonicalArray.digits, returns: number }
P: { parameters: {} }
R: '27'
```

这两个位置的权重不同。把字面量改成 `{7, 2}`，必须得到 `72`，不能套用加法的交换律给数组元素排序。

## 长度分配与单元素初始化不能合并

```java
class CanonicalArray {
    static int sized() {
        int[] values = new int[3];
        return values.length * 10 + values[0];
    }
}
```

**原样通过。** 只给出长度时，三个整数槽都以零初始化，所以返回 `30`。替换为 `new int[]{3}` 时，只有一个槽且值为 3，必须返回 `13`。空数组只可读取长度，索引越界是 Java 异常，不是返回 null 或补零。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalArray.java#CanonicalArray.sized, returns: number }
P: { parameters: {} }
R: '30'
```

## 数组失败与正常返回分开表达

```java
class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{2, 7})[-1];
        return 7;
    }
}
```

**原样通过。** 唯一正常结果是 7；仅在 `fail` 为真时发生下标越界。它不会生成一个值为 0、null 或 7 的正常分支。若改为求 `(new int[-1]).length`，异常变成负长度分配，guard 仍然相同。在入口内捕获该运行时异常则按源码执行 fallback。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalArray.java#CanonicalArray.guarded, returns: number }
P:
  parameters:
    fail: { position: 0, type: bool }
R: '7'
S: []
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'fail' }
```

本组验收是封闭 Java `int[]` 初始化、长度、按位读取和相关异常。它不把可变 Java 数组当成不可变 Vector，也不声称已覆盖数组写入、逃逸、引用比较或任意符号化的分配长度。
