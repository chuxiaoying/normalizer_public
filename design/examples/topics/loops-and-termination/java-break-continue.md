# Hello World：循环中的 continue、break 与循环后汇合

## 设计意图

区分跳过当前迭代、退出整个循环和方法返回。退出后的汇合读取只看到真正执行过的迭代；没有预处理交付的原始跳转要明确拒绝，不能当无操作。

## 输入原始代码

循环跳过前两个索引，遇到大于四的索引提前终止，其余索引累加。

```java
class BreakContinue {
    static int sum(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            if (i < 2) continue;
            if (i > 4) break;
            total = total + i;
        }
        return total;
    }
}
```

## AI 预处理结果

```diff
--- a/src/BreakContinue.java
+++ b/src/BreakContinue.java
@@ -1,11 +1,12 @@
 class BreakContinue {
     static int sum(int limit) {
         int total = 0;
-        for (int i = 0; i < limit; i++) {
-            if (i < 2) continue;
-            if (i > 4) break;
-            total = total + i;
-        }
+        /* @normalizer-ir
+           total = fold [0, limit) as i
+             when i < 2 continue
+             when i > 4 break
+             yield i
+        */
         return total;
     }
 }
```

直接提交原 Java 的 `continue` 或 `break`，会在对应关键字处要求先预处理，而不是送入内核后才报泛化循环错误。上面的显式 fold IR 保留这两个动作，运行结果不变；不把 Java 标签式跳转推断为本例的单层循环控制。

**预处理要点**：continue 和 break 都是迭代域的一部分，不能被预处理粗略改成普通筛选或截断集合。内核需要同时看到跳过、终止和已提交前缀。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/BreakContinue.java#BreakContinue.sum
  returns: number

P:
  parameters:
    limit: { position: 0, type: number }

R: |-
  CASE
    WHEN limit <= 2 THEN 0
    WHEN limit == 3 THEN 2
    WHEN limit == 4 THEN 5
    ELSE 9
  END
```

**PERS 验收要点**：循环化简必须保留控制动作对已提交前缀的影响，不能只看最终遍历集合，也不能把 break 后的元素算进结果。
