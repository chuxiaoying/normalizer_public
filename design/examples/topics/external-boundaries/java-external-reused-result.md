# Hello World：外部结果在同次执行中复用

## 设计意图

对照同一实际调用的多次读取与重新调用。复用一个已取得结果保持一个 S 回执；同参再调仍独立，数字序号排序不能反过来移动 S。两次同一端口的领号调用的结果可能不同，交叉相减不能归零；仅将第二次结果与其自身相减才归零，仍须保留先后两次调用。

## 输入原始代码

入口向外部领号服务申请一次编号，再复用结果。发号会在服务端占用一个编号，不是纯时钟观察；返回值未知，但同一次完成得到的局部值固定。

```java
class NumberService {
    private final TicketIssuer issuer;

    NumberService(TicketIssuer issuer) {
        this.issuer = issuer;
    }

    int doubled() {
        int reading = issuer.issue();
        return reading + reading;
    }
}

interface TicketIssuer {
    int issue();
}
```

## AI 预处理结果

AI 标记外部端口并显式装配入口接收者，调用、局部变量和复用关系保持原样。

```diff
--- a/src/NumberService.java
+++ b/src/NumberService.java
@@ -1,16 +1,23 @@
+import normalizer.intrinsics.N;
+
 class NumberService {
+    @N.External
     private final TicketIssuer issuer;

     NumberService(TicketIssuer issuer) {
         this.issuer = issuer;
     }

     int doubled() {
         int reading = issuer.issue();
         return reading + reading;
     }
 }

-interface TicketIssuer {
-    int issue();
+class TicketIssuer {
+    native int issue();
 }
+
+class Beans {
+    static final NumberService ENTRY = new NumberService(new TicketIssuer());
+}
```

按[标准操作与声明标记](../../../secondary/标准操作与声明标记.md)，读取时钟的设计合同是每次执行取得一条 E，不产生领号的 S。本种子验收的是领号端口及其回执复用，不验收时钟。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/NumberService.java#Beans.ENTRY.doubled
  returns: number

P:
  parameters: {}

R: $e_0 + $e_0

S:
  - call:
      external: src/NumberService.java#Beans.ENTRY.issuer.issue
      args: []
      returns: number
      result: $e_0
```

**预处理要点**

- 外部身份由 Normalizer 自身标记表达，工程不需要按 `TicketIssuer` 或具体库名做特判。
- AI 只标记边界，不把未知外部结果提前变成常量，也不丢掉同次复用关系。

**PERS 验收要点**：外部值可以未知，但同一执行中的局部数据流仍应由内核精确表达。

派生对照分别调用 `issuer.issue()` 两次。即使两个调用的参数相同，也不能合并事件：如果两次分别读到 7 和 9，返回 16；复用第一次结果则返回 14。

接收结果按真实调用的先后编号，编号是整数。派生验收将 11 次同参数调用全部保留，结果公式中 `$e_2` 要在 `$e_10` 之前；不能按数字的十进制字符串排序，也不能用公式排序反向改变 S。
