# Java：赋值是否合法与业务数值表示分开

## 设计意图

区分声明初始化、普通赋值、复合赋值、返回等上下文的 Java 源码类型规则。相同运算树不是相同赋值语义：`x += rhs` 与 `x = x + rhs` 的转换规则不同。静态非法的赋值不能因后面被覆盖、或因为业务数值只有一种类型就予以放行；通过源码检查后，业务数值也不因 Java 存储位数改变。

[本主题](README.md)

## 输入原始代码

```java
class AssignmentConversion {
    static int read() {
        int value = 99;
        value += 99L;
        return value;
    }
}
```

## AI 预处理结果

**原样通过。** 复合赋值按 [Java 复合赋值](https://docs.oracle.com/javase/specs/jls/se25/html/jls-15.html#jls-15.26.2) 的源码规则可以把计算结果转回左侧类型。将其改为简单赋值后，右侧是 `long`，不能按 [Java 赋值上下文](https://docs.oracle.com/javase/specs/jls/se25/html/jls-5.html#jls-5.2) 隐式赋给 `int`。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/AssignmentConversion.java#AssignmentConversion.read, returns: number }
P: { parameters: {} }
R: '198'
```

| 对照 | 独立预期 |
|---|---|
| 改为 `value = value + 99L` | 编译期无隐式缩窄，入口拒绝且无 PERS |
| 先做非法普通赋值，再将它覆盖 | 仍然拒绝，源码不因写入不可观察而合法 |
| 声明、数组元素初始化或返回把 `long` 隐式赋给 `int` | 相同源码赋值规则，均在入口拒绝 |
| 将同一个累加用在 `long` 声明和返回上 | 合法，业务结果仍为 `198` |
| 用 `byte value = 127; value += 1;` | 初始化和复合赋值均为合法源码；这里按已确认的精确业务数值输出 `128`，不引入机器回绕 |
| 简单赋值或返回的整数／字符字面量（可带一元正负号）转为 Java `byte` 或 `short` | 编译期只允许落在各自范围内：−128…127、−32768…32767；归一化仍使用精确业务数值。 |
| 循环计数器声明为 `final` 却执行递增，或给已有初值的 `final` 局部变量再赋值 | 均违反 Java 编译规则；确定终止不附带修改权限 |

此例只验收已列出的源码转换方向。更一般的常量表达式（包括算式和常量绑定）的缩窄证明、以 `char` 为目标的转换、引用和装箱的赋值转换、空白 `final` 的确定赋值及一般方法参数转换分别有自己的规则，不能由这些场景推断已全部支持。
