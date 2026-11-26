# Hello World：状态集合原地修改与别名观察

## 设计意图

检验原地编辑、创建副本和通过别名再观察。写回同一状态及后续读必须反映操作；变更局部副本不会自动变成共享集合的 S。

## 输入原始代码

入口修改状态集合的首项，再通过另一个局部引用读取同一位置。状态数组非空引用，但可以没有元素；越界时先抛异常，不产生写回。

```java
class StateList {
    int[] values;

    int update(int next) {
        int[] alias = values;
        values[0] = next;
        return alias[0];
    }
}
```

## AI 预处理结果

```diff
--- a/src/StateList.java
+++ b/src/StateList.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.EntryState;
+
 class StateList {
+    @EntryState
     int[] values;
```

预处理只标记集合状态根，保留原地写入、局部别名和读取顺序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/StateList.java#StateList.update, returns: number }
P:
  parameters:
    next: { position: 0, type: number }
E:
  values:
    kind: state
    source: src/StateList.java#StateList.values
    type: 'number[]'
S:
  - write: { target: 'values[0]', value: next, when: 'LENGTH(values) > 0' }
R: next
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'LENGTH(values) == 0' }
```

**验收要点**：集合别名必须观察同一状态身份，原地写入不能被复制成一个无关的新数组。

派生对照检验失败的求值次序。把写入语句改为 `values[0] = 3 / 0` 时，简单赋值先执行右值，所以无论数组是否为空，都只抛出算术异常。改为 `values[0] += 3 / 0` 时，复合赋值先读取原值：空数组先抛下标越界，非空数组才进入除零。两者都没有正常返回值（`R: ⊥`），也都没有完成写入（`S: []`）。
