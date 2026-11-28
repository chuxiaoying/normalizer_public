# Hello World：无状态实例方法入口

## 设计意图

检验无状态实例入口与闭合静态表达式之间可共享的语义。接收者无可观测字段时不应凭空生成 E；加入真实状态读取或写入就不能沿用无状态结论。

## 输入原始代码

入口是 Java 实例方法，但方法没有读取或写入对象字段。实例 receiver 不因为语法上的 `this` 自动进入状态集合。

```java
class InstanceEntry {
    int add(int value, int fee) {
        return value + fee;
    }
}
```

## AI 预处理结果

实例方法的接收者必须可从类名找到。本例构造器没有外部依赖或可观察状态，因此显式交付静态接收者；方法体不改。

```diff
--- a/src/InstanceEntry.java
+++ b/src/InstanceEntry.java
@@ -1,5 +1,7 @@
 class InstanceEntry {
+    static final InstanceEntry INSTANCE = new InstanceEntry();
+
     int add(int value, int fee) {
         return value + fee;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/InstanceEntry.java#InstanceEntry.INSTANCE.add
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    fee:
      position: 1
      type: number

R: fee + value
```

**预处理要点**：只要源码没有显式状态语义，AI 不为 `this` 添加隐含状态或动作，也不把接收者变成 P。

**PERS 验收要点**：入口形态和业务状态是两个层次，接收者路径只负责定位；读取已有状态由 E 交代，实际写入才进入 S。

## 包名不需要预处理成平铺类名

第四章把服务发现改成 `new AddOne()` 后，`Entry`、`Op` 和 `AddOne` 保留在 `demo` 包，入口为 `src/demo/Entry.java#demo.Entry.run`，执行同包接口调用得到 `x + 1`。另一个包即使也有简单类名 `Add`，也不是所选的声明。文件和包限定的类名必须同时匹配，不能从其中一个猜另一个。若词法作用域里的局部对象与同包类型同名，调用仍绑定局部对象。
这也不允许工程自行声明 `normalizer.intrinsics.N` 来替换真正的内置类型。

直接调用跨包的公开静态方法时，显式导入与完全限定名各定位自己的公开声明。若两个同名 `Tools` 分别返回 11 和 29，表达式 `Tools.change(units) * 10 + example.second.Tools.change(units)` 返回 139。类或成员没有对调用方公开时，不能只凭名字放行。若 `Tools` 实际是局部变量，则沿局部接收者调用，而不是改成导入的静态方法。
若这两个公开方法分别返回同一实参加 1 和同一实参加 2，先执行各自的方法体，再做精确归约，差是 -1；不是在链接时按名称把两个调用合并。

同包不等于同一个顶层类：`Entry.run` 可以调用另一个同包类型 `Tools` 的包可见静态方法，得到 `x + 1`；把该方法改成 `private` 后不得继续模拟。若 `Tools` 是 `Entry` 自己的嵌套类型，调用其私有辅助方法仍然有效。无包名的两个顶层类也不能互相访问私有方法。

同包调用 `protected` 静态方法时按 Java 原本的包可见性绑定；若是同一个顶层类内的两个兄弟嵌套类，则它们仍可通过源码里的真实限定名调用彼此的私有辅助方法。两种对照都返回 `value + 1`，不是放宽不同顶层类之间的私有访问。

### 入口使用已经装配好的内部依赖

[第四章的 Beans 装配](../../../chapters/04-AI救赎.md#把隐藏的执行过程写出来)提供两条入口。经 `Beans.QUOTE_CONTROLLER.quote(units)` 时，调用发生在静态块通过 `setPrice(PRICE)` 绑定的那个普通源码对象上；直接选 `Beans.PRICE.quote(units)` 时，使用同一个静态根。两者按原方法体计算 `units * 2`，不把 `PRICE` 变成入口参数、外部回执或本次新分配的对象。换成局部别名不改变绑定，改方法体为乘 3 则结果确实变成乘 3。若服务的运行时代码读一次时钟，E 恰当地记录这次读取，不把之前的装配写入算为本次 S。
