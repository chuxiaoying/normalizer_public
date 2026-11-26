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
