# Java 源码闭合：声明唯一、按所属类型绑定

## 设计意图

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

两个同名方法分属不同类型。为了让源文件边界也成为独立测试维度，把完整的 OtherPrice 声明原样移到同目录文件，不改入口、方法体或所属类型，不删未使用的方法，也不引入端口。

```diff
--- a/src/Catalog.java
+++ b/src/Catalog.java
@@ -4,6 +4,3 @@
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
| 所选 Class.method 新增有不同签名的同名方法 | 现行源码入口没有签名选择信息，须拒绝，不使用声明顺序推断。这不是 Java 语言禁用重载 |
| 所选入口被改名、旧入口不存在 | 拒绝且无 PERS，不退回其他同名方法 |
| 声明存在但实际调用的源码缺失 | 比名义歧义更晚的阶段；按[无源码合同](java-unsupported-source-boundary.md)和[非法声明、未执行 native 对照](java-invalid-declaration.md)，到达非法无源码时内核 Panic，不造 S |
