# Hello World：按相反顺序关闭嵌套任务

## 设计意图

检验栈内任务的逆序匹配和失败优先级。完整逆序、部分匹配、同号但位置错误需区分；不能把集合相同当作次序相同，也不能重排真实关闭动作。同一已取得数组的局部别名不改变位置来源；改变显式匹配字段后应只比较新字段。即使入口不使用栈空结果，已经发生的匹配失败仍可观察。不参与匹配的记录字段不改变异常；交换两数组的角色只保留空栈长度相等的关系，超额关闭方向和倒序匹配仍随角色改变。

[本主题](README.md)

## 输入原始代码

批处理先按 opened 压入所有任务，再按 closed 逐一关掉栈顶。数组及元素都不是 null，两组的长度都可以为零。

```java
import java.util.ArrayDeque;
record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        var stack = new ArrayDeque<Task>();
        for (Task task : opened) stack.push(task);
        for (Task task : closed) {
            if (stack.isEmpty() || stack.pop().id() != task.id())
                throw new IllegalArgumentException();
        }
        return stack.isEmpty();
    }
}
```

## AI 预处理结果

两段循环分别交付按序 push 和按序检查并 pop；第二段在空栈或编号不配时停止并抛出原异常。栈为局部值。输入保证所有打开操作在关闭前完成，因此内核可以把逐次弹栈归约成反向位置匹配。

```diff
--- a/src/TaskBatch.java
+++ b/src/TaskBatch.java
@@ -1,13 +1,17 @@
-import java.util.ArrayDeque;
-record Task(int id) {}
+class Task { int id; }
 class TaskBatch {
     static boolean complete(Task[] opened, Task[] closed) {
-        var stack = new ArrayDeque<Task>();
-        for (Task task : opened) stack.push(task);
-        for (Task task : closed) {
-            if (stack.isEmpty() || stack.pop().id() != task.id())
-                throw new IllegalArgumentException();
-        }
-        return stack.isEmpty();
+        /* @normalizer-ir
+           stack = fold opened from [] as (s, task) yield value.push(s, task)
+        */
+        /* @normalizer-ir
+           stack = fold closed from stack as (s, task)
+             when value.length(s) == 0 throw IllegalArgumentException
+             when value.last_or(s, task).id != task.id throw IllegalArgumentException
+             yield value.pop(s)
+        */
+        boolean empty;
+        /* @normalizer-ir empty = value.length(stack) == 0 */
+        return empty;
     }
 }
```

预处理把仅观察 id 的记录换成保留同名字段的名义载体，不改动数组位置、重复元素或字段值。局部 IR 表达两段有序折叠。每步按列出的顺序检查异常，正常时才交付下一栈；空栈已先被拦截。错配前的临时栈不会逃出本方法。[已有复合状态合同](../../../../implementation/contracts/normalized-state.md)

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Task: { fields: { id: number } }
entry: { function: src/TaskBatch.java#TaskBatch.complete, returns: bool }
P:
  parameters:
    opened: { position: 0, type: 'Task[]' }
    closed: { position: 1, type: 'Task[]' }
R: LENGTH(closed) == LENGTH(opened)
S: []
throws:
  - type: java.lang.IllegalArgumentException
    when: LENGTH(closed) > LENGTH(opened) or EXISTS (SELECT 1 FROM indices(LEAST(LENGTH(closed), LENGTH(opened))) AS i WHERE closed[i].id != opened[LENGTH(opened) - 1 - i].id)
```

**判别例**　indices(n) 枚举 0 到 n−1。打开 [1,2]、关闭 [2,1] 返回真；关闭 [1,2] 抛异常；只关闭 [2] 返回假。

当两边都为空时返回真；没有关闭且已经打开至少一个任务时返回假。重复编号按位置参与比对，不去重。两个失败检查抛出同一种无载荷异常，栈为不逃逸局部值，因此可以用存在不匹配的位置表示失败，不能反过来比较打开顺序，也不能使索引检查越出两个输入的共同前缀。
