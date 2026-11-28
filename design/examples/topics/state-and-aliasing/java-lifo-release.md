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

局部栈不逃逸，两个失败抛同一种无载荷异常。先定义两边有位置可比较的共同前缀，再用反向下标比编号。宿主代码决定是否抛异常。

```diff
-import java.util.ArrayDeque;
-
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
+        int n = opened.length;
+        int m = closed.length;
+        int mismatches;
+        /* @normalizer-ir
+           mismatches = count from [0, min(n, m)) as i
+                        where closed[i].id != opened[n - 1 - i].id
+        */
+        if (m > n) throw new IllegalArgumentException();
+        if (mismatches > 0) throw new IllegalArgumentException();
+        return m == n;
     }
 }
```

比较只读共同前缀，保留原有记录身份、字段、重复值和位置。两边同时为空时没有访问，也没有错误。

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

若超量关闭抛 `ExtraClose`、位置不匹配抛 `WrongTask`，两种已声明的 Java 异常各自保留。超量检查在前；即使同一输入也有位置不匹配，只报告 `ExtraClose`。先扣除超量路径，剩余路径才可能抛 `WrongTask`。正常域上的长度相等结果不变。
