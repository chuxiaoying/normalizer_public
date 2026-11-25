# Hello World：外部结果在同次执行中复用

## 输入原始代码

入口读取外部时钟一次，再复用同一个局部结果。外部返回值未知，但同一次调用得到的局部值固定。

```java
class ClockService {
    private final Clock clock;

    ClockService(Clock clock) {
        this.clock = clock;
    }

    int doubled() {
        int reading = clock.now();
        return reading + reading;
    }
}

interface Clock {
    int now();
}
```

## AI 预处理结果

AI 只在外部对象声明处增加 Normalizer 自身的标记，调用、局部变量和复用关系保持原样。

```diff
--- a/src/ClockService.java
+++ b/src/ClockService.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.External;
+
 class ClockService {
+    @External
     private final Clock clock;
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/ClockService.java#ClockService.doubled
  returns: number

P:
  parameters: {}

R: $e_0 + $e_0

S:
  - call:
      external: clock.now
      args: []
      returns: number
      result: $e_0
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只有 Java 字段声明，new 只增加 `@External` 和导入。 | 外部身份由 Normalizer 自身标记表达，工程不需要按 `Clock` 或具体库名做特判。 |
| old 与 new 都保留一次调用和两次局部读取。 | AI 只标记边界，不把未知外部结果提前变成常量，也不丢掉同次复用关系。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只写出 `reading + reading`，new 在 S 中绑定一次外部调用，并在 R 中复用 `$e_0`。 | 外部值可以未知，但同一执行中的局部数据流仍应由内核精确表达。 |

派生对照分别调用 `clock.now()` 两次。即使两个调用的参数相同，也不能合并事件：如果两次分别读到 7 和 9，返回 16；复用第一次结果则返回 14。
