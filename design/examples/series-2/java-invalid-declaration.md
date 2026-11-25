# Hello World：合法入口旁的非法声明

[十个业务边界案例](README.md)

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

## 构造表达式的类型参数不能被丢弃

`new java.util.ArrayList<String>()` 和 `new java.util.ArrayList<>()` 都有完整的类型语法；`<>` 是构造调用的类型推断写法，不是声明类型的一部分（[JLS 15.9](https://docs.oracle.com/javase/specs/jls/se21/html/jls-15.html#jls-15.9)）。这不表示直接源码入口已经支持通用集合构造。当前源码合同仍可要求 AI 按已支持的业务种子预处理实际集合操作。

`new java.util.ArrayList<String()` 缺少 `>`，`new java.util.ArrayList<String,>()` 缺少逗号后的类型。两者必须在源码解析阶段拒绝，即使声明不在所选入口的调用路径上；不能吞掉残缺类型再报“构造操作不支持”。解析 `new int[n]` 时，`[n]` 仍由数组构造消费，不当作声明类型的 `[]`。
