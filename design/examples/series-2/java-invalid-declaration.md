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
