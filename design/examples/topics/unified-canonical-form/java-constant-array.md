# Java 常量数组：按源码位置执行，不按元素值排序

## 设计意图

检验常量数组按源码位置、长度与边界执行。零长、首项、非零索引和越界是不同观察；构造、后续绑定和访问不能丢失实际元素类型。直接观察构造表达式的 `.length` 时，也要先执行初始化，不能只数出逗号分隔的元素。两个值相等仍各占一个位置；空数组的索引 0、Java int 的最小和最大索引均按实际长度检查，不能回绕成有效槽或在未执行读取的路径生成异常。

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

显式 `new int[]{…}` 或 `new int[n]` 不能交给 `long[]` 变量，即使后面只读取长度，且不论这是声明初始化还是后续赋值。源码的元素类型必须保留到赋值校验，不能因为两者都可能投影成业务数字数组，就把两个 Java 类型混为一个（[JLS 4.10.3](https://docs.oracle.com/javase/specs/jls/se21/html/jls-4.html#jls-4.10.3)）。

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

## 直接读取新数组的长度，先执行元素初始化

读取 `(new int[]{}).length` 得到 0；`(new int[]{2, 7, 11}).length` 得到 3。这里没有局部数组绑定，长度只取决于元素个数，不取决于元素值的大小或排序。

按 [JLS §10.6](https://docs.oracle.com/javase/specs/jls/se25/html/jls-10.html#jls-10.6)，同一初始化式内的元素从左到右求值，某个元素失败后不会继续求后面的元素，也不能读到长度。因此 `(new int[]{(new int[]{2})[1], (new int[-1]).length}).length` 首先抛出越界异常；交换两个元素后首先抛出负长度异常。若元素只在某个输入条件下越界，正常路径仍可读取长度，异常路径才执行该类型的捕获分支。 在 `{probe.read(10), fail ? (new int[]{2})[1] : 5, probe.read(30)}` 中，第一条外部调用无条件发生，第二条仅在 `fail` 为假时发生，且两条保持顺序。正常返回 3，捕获路径返回 42；不因读取长度而提前观察、折叠或丢弃调用。即使不观察元素值，`new int[]{true}` 的非法类型也必须在源入口拒绝。

## 数组失败与正常返回分开表达

```java
class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{2, 7})[-1];
        return 7;
    }
}
```

**原样通过。** 唯一正常结果是 7；仅在 `fail` 为真时发生下标越界。它不会生成一个值为 0、null 或 7 的正常分支。若改为求 `(new int[-1]).length`，异常变成负长度分配，guard 仍然相同。在入口内捕获该运行时异常则按源码执行 fallback。 已能由数组操作隐式产生的异常可按其已知 Java 类型捕获：越界可匹配 `ArrayIndexOutOfBoundsException` 或其[父类 `IndexOutOfBoundsException`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/ArrayIndexOutOfBoundsException.html)，负长度匹配 `NegativeArraySizeException`。这是类型名称解析，不额外接纳这些类的任意构造或载荷。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalArray.java#CanonicalArray.guarded, returns: number }
P:
  parameters:
    fail: { position: 0, type: bool }
R: '7'
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'fail' }
```

本组验收是封闭 Java `int[]` 初始化、长度、按位读取和相关异常。它不把可变 Java 数组当成不可变 Vector，也不声称已覆盖数组写入、逃逸、引用比较或任意符号化的分配长度。

在解析器层，`new int[][]{row, {2, 3}}` 的第一个元素仍是已有的一维数组表达式，第二个是递归的数组简写，两者次序不能颠倒。这只验收合法 Java AST，不新增二维数组构造表达式的 PERS 执行合同。
