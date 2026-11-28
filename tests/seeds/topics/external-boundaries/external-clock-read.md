# Hello World：只读时钟是环境观察

## 设计意图

跑到读时钟的那一行才取得以 Unix 纪元为起点的毫秒数。读取来自环境，不新增参数，不读执行机的真实时间，也不凭空产生 S。

## 输入原始代码

```java
class ClockRead {
    static long run(long start) {
        return System.currentTimeMillis() - start;
    }
}
```

## AI 预处理结果

```diff
--- a/src/ClockRead.java
+++ b/src/ClockRead.java
@@ -1,5 +1,6 @@
+import normalizer.intrinsics.N;
 class ClockRead {
     static long run(long start) {
-        return System.currentTimeMillis() - start;
+        return N.currentTimeMillis() - start;
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/ClockRead.java#ClockRead.run, returns: number }
P:
  parameters:
    start: { position: 0, type: number }
E:
  $a_0:
    kind: acquisition
    operation: normalizer.intrinsics.N.currentTimeMillis
    type: number
R: $a_0 - start
```

**判别例**

- 将同一次读取赋给两个变量再相减，R 可化为 0，但 E 仍有这次读取。
- 两次读取相减，R 依赖两个按实际执行顺序取得的值；两次读取的身份不同，值可以相等。既不化为 0，也不假定时间单调或差必为正。
- 开关未打开时返回原参数、没有取得时钟值。第二次读取也可以由第一次的值来控制；只有进入允许读取的条件域才可引用第二个值。
- 工程内其他同名方法按原生源码执行；未绑定的 N 不得凭拼写冒充标准操作。
- 入口还写入状态时，按 [PERS-08](../../../../design/chapters/07-PERS登场.md#pers-08-观察身份先于显示编号) 保留时钟观察和实际写入的先后。前一项写入有守卫时，只在其实际发生的域内认定这一先后关系；不从 E 或 S 的显示编号反推。
