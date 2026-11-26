# Hello World：多个入口共享状态根

## 设计意图

检验共享状态根在不同声明入口下仍有明确读取与写入来源。一个入口的写入不能在另一个入口分析时凭空发生；同一已提供入场状态才能说明观察等价。

## 输入原始代码

`increment` 修改账户状态，`read` 在另一个入口读取同一个状态根。两个入口共享类型和状态来源，但各自有独立的 PERS 入口合同。

```java
class Counter {
    int value;

    int increment() {
        value = value + 1;
        return value;
    }

    int read() {
        return value;
    }

    int incrementThenRead() {
        increment();
        return read();
    }
}
```

## AI 预处理结果

```diff
--- a/src/Counter.java
+++ b/src/Counter.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.EntryState;
+
 class Counter {
+    @EntryState
     int value;
```

预处理只标记状态根，保留两个入口的原生方法体。

## `increment` 的归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Counter.java#Counter.increment
  returns: number

P:
  parameters: {}

E:
  value:
    kind: state
    source: src/Counter.java#Counter.value
    type: number

S:
  - write:
      target: value
      value: value + 1

R: value + 1
```

## `read` 的归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Counter.java#Counter.read
  returns: number

P:
  parameters: {}

E:
  value:
    kind: state
    source: src/Counter.java#Counter.value
    type: number

R: value
```

**PERS 验收要点**：跨入口状态保留由共享状态根表达，不能把每个入口的 receiver 当成互不相关的普通输入，也不能把一次入口调用的结果偷偷写入另一个入口的 P。


## 同一实例上的调用组合

单独的入口归一化只能检查共同的状态来源。此入口在同一 receiver 上先调用 increment，再调用 read，要求后者观察写后的状态；不会把两次调用当成两个入口快照。

```yaml
schema: normalizer.pers/1
entry: { function: src/Counter.java#Counter.incrementThenRead, returns: number }
P: { parameters: {} }
E:
  value: { kind: state, source: 'src/Counter.java#Counter.value', type: number }
S:
  - write: { target: value, value: value + 1 }
R: value + 1
```

派生对照在写入前调用 read，再执行同样的写入。初始值为 7 时，两者的写入结果都是 8，但返回值分别为 8、7。
