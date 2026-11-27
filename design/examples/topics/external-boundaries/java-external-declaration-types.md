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

该注入字段有已确认的外部接口签名。用标准标记和静态装配保留同一接收者，不变更业务方法体或回执字段类型。

```diff
--- a/src/ExternalTypeDemo.java
+++ b/src/ExternalTypeDemo.java
@@ -1,10 +1,19 @@
+import normalizer.intrinsics.N;
+
 class ExternalTypeDemo {
+    @N.External
     private final Records port;
     ExternalTypeDemo(Records port) { this.port = port; }
     String label(int id) {
         Packet packet = port.lookup(id);
         return packet.label;
     }
 }
-interface Records { Packet lookup(int id); }
+class Records {
+    native Packet lookup(int id);
+}
 class Packet { boolean approved; String label; }
+
+class Beans {
+    static final ExternalTypeDemo ENTRY = new ExternalTypeDemo(new Records());
+}
```

`Beans.ENTRY` 定位已装配实例；`native` 端口声明交付原签名，按[外部端口合同](../../../chapters/04-AI救赎.md#ai-08-外部端口要有真实合同)处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/ExternalTypeDemo.java#Beans.ENTRY.label, returns: text }
P:
  parameters:
    id: { position: 0, type: number }
types:
  Packet:
    fields: { approved: bool, label: text }
S:
  - call: { external: src/ExternalTypeDemo.java#Beans.ENTRY.port.lookup, args: [id], result: $e_0, returns: Packet }
R: $e_0.label
```

| 改变输入的维度 | 独立验收 |
|---|---|
| 改读同一记录的布尔字段 | 同一 S 和记录类型，R 指向布尔字段，入口返回 bool |
| 源码不再调用端口 | 不产生 S，也不发布未取得的记录类型；R 是入口直接给出的文字 |
| 注入字段所声明的端口类型不存在 | 无法证明端点和签名，预处理诊断且无 PERS |
| 给注入字段换成有实际业务源码、没有外部合同的类 | 不能把一个本地方法仅靠标记伪装成 S；端口须有明确的外部声明与接收者绑定 |
| 遮蔽或重绑导致实际接收者不再是已声明端口 | 按 [AI-03、AI-08](../../../chapters/04-AI救赎.md#ai-08-外部端口要有真实合同) 解析实际接收者；不能继承旧标记，也不能凭字段是否为 final、静态或经构造器赋值，就推断端口合同。 |
| 普通形参与字段同名、异常和取得域变化 | 复用[标量端口及同名接收者对照](java-external-scalar.md)、[异常传播](java-exception-propagation.md)和[有条件外部回执](../unified-canonical-form/java-exception-payload.md)；不重复测试 |

本例只验收可解析的端口签名与声明中的记录字段；未证明的接收者不得当成已闭合端口。

在未被遮蔽的实例方法中，`this.port.lookup(id)` 和 `port.lookup(id)` 都引用同一个已注入的字段，S 的端点和已取得的回执不变。将方法返回和记录字段的 `String` 全称写作 `java.lang.String` 也只改变源码拼写，PERS 仍是同一个 `text`。

签名中的类型与发布时的领域分别验收：int 和 double 都是 number，boolean 是 bool，`Row[]` 仍是同一名义记录的数组，不能把数组或记录降成标量；这些规则不依赖哪一项在所选方法体被读取。
