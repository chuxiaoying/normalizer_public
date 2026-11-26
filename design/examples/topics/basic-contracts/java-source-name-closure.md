# Java 源码闭合：声明唯一、按所属类型绑定

## 设计意图

默认构造器只闭合无参构造。源码没有声明对应有参构造时，不能把传入参数忽略后在内核创建对象。

区分合法的同名方法、不能唯一绑定的声明和根本不存在的入口。分开改变所属类型、顶层与嵌套命名空间、声明签名和入口选择。任何一项不能确定，都不可通过声明顺序猜一个结果。

## 输入原始代码

```java
class Catalog {
    static int price() { return Price.price(); }
}
class Price {
    static int price() { return 12; }
}
class OtherPrice {
    static int price() { return 99; }
}
```

## AI 预处理结果

**原样通过。** 两个同名方法分属不同类型，源码足以确定调用。不为制造测试维度而移动声明或重写方法。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Catalog.java#Catalog.price, returns: number }
P: { parameters: {} }
R: '12'
```

| 变化 | 独立预期 |
|---|---|
| 只把调用所属类型换成 OtherPrice | 改为 99，不能按方法名合并 |
| 同一文件或不同文件的两个顶层声明具有相同类型名 | 不能选取先读到的实现；拒绝且无 PERS |
| 同一类型声明两个同名嵌套类型，即使入口不用它们 | 不是两个可独立绑定的类型；拒绝且无 PERS |
| 所选 Class.method 新增有不同签名的同名方法 | 未说明签名时拒绝歧义，不使用声明顺序推断；补上工程相对路径和原生参数类型后可精确选择。这不是 Java 语言禁用重载 |
| 所选入口被改名、旧入口不存在 | 拒绝且无 PERS，不退回其他同名方法 |
| 声明存在但实际调用的源码缺失 | 比名义歧义更晚的阶段；按[无源码合同](java-unsupported-source-boundary.md)和[非法声明、未执行 native 对照](java-invalid-declaration.md)，到达非法无源码时内核 Panic，不造 S |

同一类型内的两个字段也必须能唯一声明；未使用类型中的 `int code; int code;` 是非法 Java，不能只因为不会执行到就选其中一个，也不输出 PERS。

## 只改变文件布局的独立对照

这个对照修改的是生产输入布局，不是 AI 预处理。把 `OtherPrice` 的完整声明放进同目录文件，所选入口与绑定不变，仍返回 `12`。

```diff-example
--- a/src/Catalog.java
+++ b/src/Catalog.java
@@ -1,9 +1,6 @@
 class Catalog {
     static int price() { return Price.price(); }
 }
 class Price {
     static int price() { return 12; }
 }
-class OtherPrice {
-    static int price() { return 99; }
-}
--- /dev/null
+++ b/src/OtherPrice.java
@@ -0,0 +1,3 @@
+class OtherPrice {
+    static int price() { return 99; }
+}
```
