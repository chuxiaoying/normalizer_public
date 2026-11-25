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

**正向对照**　只把 helper 名换成 booleanValue 后，Java 声明合法。选定入口的理想结果如下；helper 不可达，不要求为合法的 native 声明补造实现。

```java
class TicketCode {
    static int read(int value) { return value; }
    static native int booleanValue();
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/TicketCode.java#TicketCode.read, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: value
```
