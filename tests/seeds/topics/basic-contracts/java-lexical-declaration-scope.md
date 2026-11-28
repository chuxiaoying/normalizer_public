# Java：局部声明重名要按实际作用域判定

## 设计意图

同一个名字能否再次声明取决于此前绑定是否仍在作用域内，不取决于源码是否实际运行。形参与可见外层局部变量不能被重叠的局部、循环变量或捕获参数重复声明；互不重叠的代码块可以复用名称，局部变量也可以遮蔽字段。入口验证不要把合法遮蔽当成重复，也不要跳过未调用方法。

[本主题](README.md)

## 输入原始代码

```java
class LexicalScope {
    int tally;
    int read(boolean choose, int input) {
        int tally = input;
        if (choose) {
            int branch = tally + 1;
            return branch;
        } else {
            int branch = tally + 2;
            return branch;
        }
    }
}
```

## AI 预处理结果

只显式装配原入口的接收对象，使其能由类名找到；不改写方法体。两个 `branch` 分属不重叠的分支，`tally` 读取局部绑定而不是同名字段，不增加 E。反例中的重叠声明应保留源码定位并在入口拒绝。

```diff
--- a/src/LexicalScope.java
+++ b/src/LexicalScope.java
@@ -1,13 +1,14 @@
 class LexicalScope {
+    static final LexicalScope INSTANCE = new LexicalScope();
     int tally;
     int read(boolean choose, int input) {
         int tally = input;
         if (choose) {
             int branch = tally + 1;
             return branch;
         } else {
             int branch = tally + 2;
             return branch;
         }
     }
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/LexicalScope.java#LexicalScope.INSTANCE.read, returns: number }
P:
  parameters:
    choose: { position: 0, type: bool }
    input: { position: 1, type: number }
R: CASE WHEN choose THEN input + 1 ELSE input + 2 END
```

| 对照 | 独立预期 |
|---|---|
| 离开子块后复用该块的局部名 | 两个作用域不重叠；返回外部新绑定的值 |
| 同一块重复写局部名，或内层块声明与可见外部局部/形参同名 | 非法 Java；入口拒绝，不输出 PERS |
| 循环变量或捕获参数与可见形参同名 | 非法 Java；也不能只因方法未调用而跳过 |
