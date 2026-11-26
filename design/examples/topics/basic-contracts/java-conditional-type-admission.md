# 有条件值按所在位置和目标类型检查

## 设计意图

在真假两侧都可达的入口中，分别改变候选返回值的类型和所在臂。相同业务标量的合法选择保留条件；本例的目标为 `int`，布尔值或整条序列不能因交换臂而转换为 `int`。无法闭合的类型组合要报告实际冲突，不应在构造诊断时触发内核内部异常。

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

这些不合法的标量／序列组合在模拟前拒绝。此处验收的是已界定的类型规则，不宣称取代整个 Java 编译器。非法无源码调用的 [专用 Panic 合同](java-unsupported-source-boundary.md) 不受影响。

### 静态校验不依赖执行路径

入口先绑定所有源码方法并检查静态类型，再让内核沿业务可达路径执行。即使方法没有被调用，或者一条条件分支可以被常量折叠，也不能跳过编译检查。合法但未执行的代码不增加 PERS 效果；不合法的代码则不会进入模拟执行。

| 输入维度 | 独立验收 |
|---|---|
| 未调用的 helper 合法，或入口使用合法恒真布尔条件 | 照常生成 PERS；未调用方法不增加交互，恒真条件折叠为选中的整数 |
| 整数或数组作为普通 Java 条件 | 不是布尔条件，入口拒绝；Java 没有隐式真假转换 |
| `int` 方法返回 `true`，或给局部整数变量赋布尔值 | 入口按声明类型拒绝，不依赖实际调用和参数 |
| 值表达式引用没声明的变量 | 在入口名称绑定时拒绝，而非沿执行路径找到变量时才失败 |
| 恒真条件把不合法的返回藏进另一臂 | 整个条件表达式仍须具备合法类型，不能折叠后放行 |

独立对照 `if (false) return true;` 和 `if (false) return 3 / value;`：前者即使走不到也违反整数返回声明；后者类型合法且不会真正执行，因此即使 `value = 0` 也只返回后面的 7，不发布异常。

这里测试的是普通 Java 表达式的静态规则，不把合法输入上可能发生的越界、空引用或业务异常误当成编译错误。

### Java 引用目标：两臂按目标转换，不要求彼此同域

按 [JLS §15.25.3](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.25.3)、[§5.2](https://docs.oracle.com/javase/specs/jls/se25/html/jls-5.html#jls-5.2) 和 [§14.17](https://docs.oracle.com/javase/specs/jls/se25/html/jls-14.html#jls-14.17)，引用条件表达式位于返回或赋值位置时，目标类型传到各臂。例如目标确为 `java.lang.Object`，可以分别将 `1` 和 `true` 装箱再向上转换，`null` 也可直接赋给它。这不意味着 `int` 可以接收 `true`，也不意味着任意同名 `Object` 都是该根类型。

| 输入维度 | 独立验收 |
|---|---|
| 未被执行的 `Object unused(boolean b) { return b ? 1 : true; }`，或等价的两个分支返回 | 源码合法；原入口完整 PERS 不变 |
| 使用全限定根类型、在引用条件的臂里嵌套条件，或在局部变量的声明及赋值位置提供根类型目标 | 目标沿条件臂传递，原入口完整 PERS 不变 |
| 项目自行定义 `Object`，而两个臂是不能转换为该类的原生数值和布尔 | 不猜为 `java.lang.Object`，入口拒绝；明确写全限定根类型则合法 |
| 目标为 `Object`，但条件是数值，某一臂引用未声明的名称，或某一臂没有返回值 | 入口拒绝；根类型不能把条件变成布尔值、补齐名称或给 `void` 造值 |

上述材料只验收源码准入，不声明条件为异型对象的值能够成为公开 PERS 或作为内核运行时值执行。预处理原样保留这些未执行的合法辅助方法，不替换分支也不扩大公开类型。
