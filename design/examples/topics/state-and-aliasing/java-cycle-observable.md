# Hello World：状态内部循环与标量出口

## 设计意图

内部出口检查另行比较共享叶子、借入的入口引用和空边：共享节点已走完时不是环，当前路径回到祖先才是环。这个用例只验收环检测，不把尚未渲染的内部对象引用当成最终 PERS。

公开身份只对应已声明的直接取得点，不由嵌套路径推断。CASE 身份比较的每一臂都要有身份或 null 证据；不能用已知臂掩盖未取得的另一臂。

区分内部对象图有循环与对外发布循环结构。读出有限路径的标量可以完成，局部自引用不凭空生成 T；直接发布无法闭合的循环对象不能伪造树。两个对象形成回指时，每次读取仍沿真实对象寻找最近字段；两个边共享一个没有回指的子对象时，改一个别名、经另一别名读取仍看到同一写入。

## 输入原始代码

root 和所有被读取的 next 均非空，允许回指已访问节点。入口只观察有限路径上的标量，不返回整个对象图。

```java
class Node { Node next; int value; }
class CycleState {
    Node root;
    int read() { return root.next.value; }
}
```

## AI 预处理结果

```diff
--- a/src/CycleState.java
+++ b/src/CycleState.java
@@ -1,5 +1,17 @@
+import normalizer.intrinsics.N;
+
 class Node { Node next; int value; }
 class CycleState {
+    @N.EntryState
     Node root;
     int read() { return root.next.value; }
 }
+
+class Beans {
+    static final CycleState ENTRY = new CycleState();
+    static final Node ROOT = new Node();
+    static {
+        ROOT.next = ROOT;
+        ENTRY.root = ROOT;
+    }
+}
```

已装配实例由 `Beans.ENTRY` 定位。初始化只是装配，不替换这次入口分析时已声明的状态快照。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
types:
  Node: { fields: { next: Node, value: number } }
entry: { function: src/CycleState.java#Beans.ENTRY.read, returns: number }
P: { parameters: {} }
E: { root: { kind: state, source: 'src/CycleState.java#Beans.ENTRY.root', fields: { next: Node, value: number } } }
R: root.next.value
```

例如 root.next = root 时仍应成功；这和完整返回可达循环对象所触发的出口拒绝是两条不同的合同。

## 实际构造环，只输出标量

这里的环不是对输入的假设：新对象的 next 明确指向它自己。读取只走一次 next，不会对整个对象图做投影。

```java
class Node { Node next; int value; }
class LocalCycle {
    static int readSelf() {
        Node root = new Node();
        root.value = 7;
        root.next = root;
        return root.next.value;
    }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/LocalCycle.java#LocalCycle.readSelf, returns: number }
P: { parameters: {} }
R: '7'
```

## 对照：完整出口含环

源程序本身正常返回对象；失败是归一化不能输出有限、完整的内容，不是源程序抛异常。测试要求诊断能区分循环出口和一般性输入准入错误。

```java
class Node { Node next; int value; }
class LocalCycle {
    static Node exportSelf() {
        Node root = new Node();
        root.value = 7;
        root.next = root;
        return root;
    }
}
```

```text
ABORT

exportSelf:
  reason: cyclic content at the return boundary
  diagnostic: NORM-CYCLIC-CONTENT-EXIT
```
