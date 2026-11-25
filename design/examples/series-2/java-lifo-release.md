# Hello World：按相反顺序关闭嵌套任务

[十个业务边界案例](README.md)

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
 record Task(int id) {}
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

局部 IR 展示本例需要的有序折叠语义。每步按列出的顺序检查异常，正常时才交付下一栈；空栈已先被拦截。错配前的临时栈不会逃出本方法。[已有复合状态合同](../../../implementation/contracts/normalized-state.md)

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
R: LENGTH(opened) == LENGTH(closed)
throws:
  - type: java.lang.IllegalArgumentException
    when: |-
      LENGTH(closed) > LENGTH(opened) OR EXISTS (
        SELECT 1 FROM indices(LEAST(LENGTH(opened), LENGTH(closed))) AS i
        WHERE closed[i].id != opened[LENGTH(opened) - 1 - i].id
      )
```

**判别例**　indices(n) 枚举 0 到 n−1。打开 [1,2]、关闭 [2,1] 返回真；关闭 [1,2] 抛异常；只关闭 [2] 返回假。
