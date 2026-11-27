# Hello World：合法入口旁的非法声明

## 设计意图

区分非法声明、合法未调用 native 和实际执行到无源码。坏声明不能因未调用而忽略，应报告原文件位置；合法未调用不产生 S，实际无源码则由内核 Panic。

[本主题](README.md)

## 输入原始代码

选择 TicketCode.read；同一个编译单元里有一个未调用的 native helper。

```java
class TicketCode {
    static int read(int value) { return value; }
    static native int return();
}
```

## AI 预处理结果

原样交给源码准入检查。return 位于方法名位置，是完整的保留字 token。保留原文件，使诊断能指向第二个声明；自动删除未调用的方法或擅自改名会掩盖输入错误。

## 归一化最终结果

目标是让该输入在解析 Java 声明时被拒绝。stdout 为空，stderr 指向第二个声明的非法方法名，而不是把问题报告为所选入口缺少方法体。

```text
PREPROCESS_REQUIRED

read:
  location: src/TicketCode.java#TicketCode.return
  reason: reserved keyword used as a Java method name
```

**正向对照**　只需把 helper 名换成 booleanValue，Java 声明就合法。下例把选定入口写作 readValid，以区分两个验收入口；它的函数体没有变化。helper 不可达，不需要补造实现。如果执行确实走到没有合法 external 合同的 native 调用，内核必须 Panic，不输出 PERS。

```java
class TicketCode {
    static int readValid(int value) { return value; }
    static native int booleanValue();
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/TicketCode.java#TicketCode.readValid, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: value
```

## 合法的嵌套声明

合法的内部类不应被跳过。显式选择完整限定名时，入口必须来自同一源码文件中的那个声明。若把嵌套方法名换成保留字，诊断也必须指回文件中的原行列，而不是内层片段的相对位置。

```java
class TicketCode {
    static class Nested {
        static int readNested(int value) { return value + 1; }
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/TicketCode.java#TicketCode.Nested.readNested, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: value + 1
```

保留字也不能做类型名。即使选中的 `TicketCode.readValid` 合法且没引用旁边的声明，源码入口仍应在解析阶段拒绝顶层 `class return`、内部 `interface while` 或 `record null`，不生成 PERS。完整名字 `returnValue` 并不是保留字，作为未调用的内部类不改变原来的 `R: value`。

## 构造表达式的类型参数不能被丢弃

`new java.util.ArrayList<String>()` 和 `new java.util.ArrayList<>()` 都有完整的类型语法；`<>` 是构造调用的类型推断写法，不是声明类型的一部分（[JLS 15.9](https://docs.oracle.com/javase/specs/jls/se21/html/jls-15.html#jls-15.9)）。这不表示直接源码入口已经支持通用集合构造。如果所选入口确实依赖该宿主库，就应按关系操作的业务含义交付，而非让内核执行 JDK 集合实现。

`new java.util.ArrayList<String()` 缺少 `>`，`new java.util.ArrayList<String,>()` 缺少逗号后的类型。两者必须在源码解析阶段拒绝，即使声明不在所选入口的调用路径上；不能吞掉残缺类型再报“构造操作不支持”。 同一块中，无条件 return 后还有语句，同样在源码入口拒绝，包括未调用方法中的代码。只退出一个 if 分支不代表整个外层块退出，其他路径照常执行。解析 `new int[n]` 时，`[n]` 仍由数组构造消费，不当作声明类型的 `[]`。

## 泛型声明与类型参数写法不能混为一谈

泛型声明的 `<T>` 声明类型变量，构造表达式的类型参数则引用或推断具体类型。语法合法的工程内泛型应按自己的声明和绑定判定，不因出现 `<T>` 就要求预处理；使用位置若绑定到无法交付的类型，才按实际缺口拒绝。局部映射还须符合分析副本的约定载体和关系指令合同，不能因为宿主泛型写法合法就放行任意 JDK 集合。

对源码级 Unicode 转义，入口先按 Java 词法解释，再检查声明和语句。合法的转义换行会结束行注释，后续 `result = result + 1` 不是注释的一部分，故本例返回 `value + 1`，不是拒绝源码或返回原值。

即使入口本身合法，未调用方法内缺失控制条件右括号或缺失数字的整数进制前缀（`0x`）也要在源码入口拒绝，不输出 PERS。反之，`/* @normalizer-irSuffix ordinary trivia */` 不是完整标记，只是普通注释；它不能改变 `readValid` 返回原入参的结果。

## 合法词法对照

完整标识符既可以用美元符号起头，也可以把它放在名字中间或末尾。给局部变量 `$left` 和 `right$` 分别绑定 1 和 2，再加到原入参上，结果是 `value + 3`。这不是新的业务入参。Java 保留字则不允许绑定局部，无论它出现在所选方法、同工程未调用的方法，还是逗号后的第二个声明。入口应在解析阶段拒绝，不让后续执行决定非法源码是否能产出 PERS。

圆括号不会把普通名称变成类型转换。给 `intValue`、`int_value`、`int$value` 依次绑定 1、2、3，再把三个括号表达式加到入口的 `value`，应归约为 `value + 6`。这里只观察原有 P 和 R，不改变公开签名，也不引入 E 或 S。

文件末尾的行注释即使没有换行也应结束在 EOF，不改变已经闭合的类或方法。相对地，字符串内的转义引号和 `/* } */ //` 都是实际文本，不是结束字符串、闭合类体或开启注释的记号；前面的非 ASCII 字符也完整保留。这一对照直接比较输出文本，而不是只检查包含这段字符串的源码能通过。

还有一组合法声明对照：选中类为 `final`，所选整数方法为 `protected static strictfp`，同文件未使用的类含 `transient` 整数字段。这些修饰符不能吞掉下一记号或改变所选方法的 `R: value`；不因此声称支持对象序列化。

原文件的 CRLF 和由 Unicode 转义生成的 CRLF 都是一次换行。源码入口诊断若指向下一行首个记号，应映射回原文件的记号起点，不落到 LF 字节或构成换行的转义上。

已知新建对象的类型测试比较真实声明，不靠变量名称。把 `new OtherCode()` 绑定到 `Object code`，是合法 Java；`code instanceof TicketCode` 为假，原输入照常返回。反过来，新建的 TicketCode 一定满足 `instanceof java.lang.Object`。这两个对照都不改变 P，也不创造 E 或 S。

闭合的双参数泛型积在实例化后仍按构造器赋值绑定字段：即使先声明 second、再声明 first，`Pair<String,java.lang.String>("LEFT", "right")` 的 first 仍取第一个参数，second 仍取第二个。两种 String 拼写不改变值域，按 first、second 拼接的结果是 `"LEFTright"`，不观察对象身份、不产生 S。
