# Hello World：状态集合原地修改与别名观察

## 设计意图

已声明的非空状态数组可以直接或通过别名读长度。两种写法都观察同一个 E 根，包括长度为零的情况；读取长度不读某一索引，不产生写回或越界异常。 无局部遮蔽时，直接写 `values.length` 与 `this.values.length` 观察同一状态；若入口形参也叫 values，未限定名称改为绑定该形参，只观察 P，绝不从同名状态猜长度。

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
@@ -1,9 +1,19 @@
+import normalizer.intrinsics.N;
+
 class StateList {
+    @N.EntryState
     int[] values;

     int update(int next) {
         int[] alias = values;
         values[0] = next;
         return alias[0];
     }
 }
+
+class Beans {
+    static final StateList ENTRY = new StateList();
+    static {
+        ENTRY.values = new int[0];
+    }
+}
```

预处理标记集合状态根并装配接收者，保留原地写入、局部别名和读取顺序。

已装配实例由 `Beans.ENTRY` 定位。初始化只是装配，不替换这次入口分析时已声明的状态快照。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/StateList.java#Beans.ENTRY.update, returns: number }
P:
  parameters:
    next: { position: 0, type: number }
E:
  values:
    kind: state
    source: src/StateList.java#Beans.ENTRY.values
    type: 'number[]'
S:
  - write: { target: 'values[0]', value: next, when: '0 < LENGTH(values)' }
R: next
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: '0 == LENGTH(values)' }
```

**验收要点**：集合别名必须观察同一状态身份，原地写入不能被复制成一个无关的新数组。

派生对照检验失败的求值次序。把写入语句改为 `values[0] = 3 / 0` 时，简单赋值先执行右值，所以无论数组是否为空，都只抛出算术异常。改为 `values[0] += 3 / 0` 时，复合赋值先读取原值：空数组先抛下标越界，非空数组才进入除零。两者都没有正常返回值，也都没有完成写入（不写 S）。

如果先成功执行 `values[0] = next`，再经别名执行 `alias[1] = 3 / 0`，第二条语句不会写回，第一条已经完成的写入也不会撤销。数组为空时，第一条语句先抛越界；数组非空（包括仅有一个元素）时，第二条先求右值并抛算术异常。两组输入均无正常返回。

在已标记的 values 之外增加一个未标记、未读取的普通 `int audit` 字段，不获得新的 E 状态；values 的原更新、索引异常和返回值都不变。

仅把启动时的 `new int[0]` 换成 `new int[257]`，分析入口读取的仍是当次的状态数组。若入口只读它的长度，R 仍引用 E 中的长度，不替换为 257。证明装配时只需确认这个字面量长度非负，不需要模拟分配容量。
