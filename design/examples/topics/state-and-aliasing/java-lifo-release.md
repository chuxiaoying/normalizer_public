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

预处理保留原记录和 id 的绑定，不改动数组位置、重复元素或字段值。局部 IR 表达两段有序折叠。每步按列出的顺序检查异常，正常时才交付下一栈；空栈已先被拦截。错配前的临时栈不会逃出本方法。[关系代数的有序集合归约](../../../chapters/05-关系代数.md)

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
throws:
  - type: java.lang.IllegalArgumentException
    when: LENGTH(closed) > LENGTH(opened) OR 0 < COALESCE(SUM(SELECT 1 FROM indices(LEAST(LENGTH(closed), LENGTH(opened))) AS i WHERE closed[i].id != opened[LENGTH(opened) - 1 - i].id), 0)
```

**判别例**　indices(n) 枚举 0 到 n−1。打开 [1,2]、关闭 [2,1] 返回真；关闭 [1,2] 抛异常；只关闭 [2] 返回假。

当两边都为空时返回真；没有关闭且已经打开至少一个任务时返回假。重复编号按位置参与比对，不去重。两个失败检查抛出同一种无载荷异常，栈为不逃逸局部值，因此可以用存在不匹配的位置表示失败，不能反过来比较打开顺序，也不能使索引检查越出两个输入的共同前缀。

位置投影必须区分列表来源、索引和字段名：对索引做等价化简不能改变来源或字段；只产生索引列表的节点也不等于读取某条记录字段。不由最终 SQL 的外观倒推列表来源。

共同前缀的长度与两边传入顺序无关，取两份不同输入的较小长度时必须仍可见两份输入；同一输入取两次最小值则可直接退化为该输入长度。比较共同前缀不交换异常判断中打开组和关闭组的业务角色。

`Task[]` 和 `Completed[]` 是不同声明，不能因都叫 `id` 就合并行来源。若原程序确实比较两者各自的已解析整数标识，仍须分别保留投影和位置，并证明比较只在共同前缀发生。本例的同类型声明不是规则外的额外准入条件。
