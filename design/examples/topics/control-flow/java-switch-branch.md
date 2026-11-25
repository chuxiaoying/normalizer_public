# Hello World：switch 预处理为原生 if 分支

## 输入原始代码

入口按代码选择固定结果。当前 Java 规范子集不能直接闭合 `switch`，但它的业务语义可以保持为原生 Java 的有序 `if` 分支。

```java
class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1:
                return 10;
            case 2:
                return 20;
            default:
                return 0;
        }
    }
}
```

## AI 预处理结果

AI 只把当前工程不能闭合的 `switch` 片段改写为语义等价的原生 Java 条件，保留入口、参数和返回结构。

```diff
--- a/src/SwitchMode.java
+++ b/src/SwitchMode.java
@@ -1,12 +1,7 @@
 class SwitchMode {
     static int classify(int code) {
-        switch (code) {
-            case 1:
-                return 10;
-            case 2:
-                return 20;
-            default:
-                return 0;
-        }
+        if (code == 1) return 10;
+        if (code == 2) return 20;
+        return 0;
     }
 }
```

预处理没有引入库特判或共享控制流语法，Normalizer 接收到的是仍然合法的 Java。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/SwitchMode.java#SwitchMode.classify
  returns: number

P:
  parameters:
    code:
      position: 0
      type: number

R: |-
  CASE
    WHEN code == 1 THEN 10
    WHEN code == 2 THEN 20
    ELSE 0
  END
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 使用 `switch`，new 只把同一方法体改成有序 `if`，入口和返回值不变。 | AI 预处理只消除工程当前无法解析的语言片段，交付仍属于原语言的最小改写。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 的多个 `case` 出口，new 归一化为按源码顺序排列的 CASE。 | 分支归一化由内核负责，预处理不提前计算或改写成共享扩展语法。 |
