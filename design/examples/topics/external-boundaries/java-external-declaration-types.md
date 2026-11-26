# Java 外部端口的声明与类型证据

## 设计意图

分别改变接收者声明、返回字段类型与实际调用是否发生。已有合法端口才能发布 S；同一回执的文本、布尔字段须沿明确的返回声明投影，而非从字段名猜类型。不把声明当成调用，也不把无法闭合的类型当成空结果。

## 输入原始代码

```java
class ExternalTypeDemo {
    private final Records port;
    ExternalTypeDemo(Records port) { this.port = port; }
    String label(int id) {
        Packet packet = port.lookup(id);
        return packet.label;
    }
}
interface Records { Packet lookup(int id); }
class Packet { boolean approved; String label; }
```

## AI 预处理结果

该注入字段有已确认的外部接口签名。只补标准标记，不变更接收者、方法体或字段类型。

```diff
--- a/src/ExternalTypeDemo.java
+++ b/src/ExternalTypeDemo.java
@@ -1,3 +1,4 @@
 class ExternalTypeDemo {
+    @normalizer.annotations.External
     private final Records port;
     ExternalTypeDemo(Records port) { this.port = port; }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/ExternalTypeDemo.java#ExternalTypeDemo.label, returns: text }
P:
  parameters:
    id: { position: 0, type: number }
types:
  Packet:
    fields: { approved: bool, label: text }
S:
  - call: { external: port.lookup, args: [id], result: $e_0, returns: Packet }
R: $e_0.label
```

| 改变输入的维度 | 独立验收 |
|---|---|
| 改读同一记录的布尔字段 | 同一 S 和记录类型，R 指向布尔字段，入口返回 bool |
| 源码不再调用端口 | 不产生 S，也不发布未取得的记录类型；R 是入口直接给出的文字 |
| 注入字段所声明的端口类型不存在 | 无法证明端点和签名，预处理诊断且无 PERS |
| 给注入字段换成有源码实现的类，而非端口接口 | 不能把一个本地方法仅靠标记伪装成 S；当前 Java 边界声明须给出端口接口 |
| 普通形参与字段同名、异常和取得域变化 | 复用[标量端口及同名接收者对照](java-external-scalar.md)、[异常传播](java-exception-propagation.md)和[有条件外部回执](../unified-canonical-form/java-exception-payload.md)；不重复测试 |

这里只验收已获准注入字段的接口签名与标量记录字段，不为未裁决的可变端口或任意嵌套返回扩张准入。
