# Hello World：先完整取号，再返回第一项

## 设计意图

对比先完整投影后取第一项与惰性取首项。后续行为空也可在投影中先失败；集合全空时是索引异常，不能见到索引零就跳过其他行。

[本主题](README.md)

## 输入原始代码

rows 可以是空数组。数组及其中每一行都不是 null，按数组顺序读取时，行内的 ticket 可以是 null。

```java
class Ticket { int id; }
class Row { Ticket ticket; }
class Batch {
    static int firstId(Row[] rows) {
        int[] ids = new int[rows.length];
        for (int i = 0; i < rows.length; i++) ids[i] = rows[i].ticket.id;
        return ids[0];
    }
}
```

## AI 预处理结果

在 `Row.ticket` 上用 `@N.Nullable` 声明该字段可空。局部有序投影替换 ids 的分配和填充，依次读取每一行的 ticket.id，遇到空引用立即抛出 NPE，成功时 ids 为完整的有序数列。保留原 return。数组资源耗尽在本例观察范围之外。

```diff
--- a/src/Batch.java
+++ b/src/Batch.java
@@ -1,9 +1,13 @@
+import normalizer.intrinsics.N;
+
 class Ticket { int id; }
-class Row { Ticket ticket; }
+class Row { @N.Nullable Ticket ticket; }
 class Batch {
     static int firstId(Row[] rows) {
-        int[] ids = new int[rows.length];
-        for (int i = 0; i < rows.length; i++) ids[i] = rows[i].ticket.id;
+        int[] ids;
+        /* @normalizer-ir
+           ids = from rows as row yield row.ticket.id
+        */
         return ids[0];
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Ticket: { fields: { id: number } }
  Row: { fields: { ticket: 'Ticket?' } }
entry: { function: src/Batch.java#Batch.firstId, returns: number }
P:
  parameters:
    rows: { position: 0, type: 'Row[]' }
R: FIRST(r.ticket.id FROM rows AS r)
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'LENGTH(rows) == 0' }
  - { type: java.lang.NullPointerException, when: 'EXISTS (SELECT 1 FROM rows AS r WHERE r.ticket IS NULL)' }
```

完整取号先完成，成功后才取索引。这里的 FIRST 只是正常结果的显示，不能移除全数组的缺失检查。空输入已蕴含没有缺失行，故其越界条件无需再写一条否定检查。

**判别例**　首行 id 为 21、第二行 ticket 为空时必须抛 NPE；只有首行时返回 21；空数组在最终索引处越界。

局部 `Row[] same = rows` 只绑定同一个已取得的数组；随后通过 `same` 做完整投影，P、R、异常条件仍指向原来的 `rows`，不新增参数、读取或副作用。别名的组件声明仍是同一个 `Row`，不能只凭内部容器形状推断。
