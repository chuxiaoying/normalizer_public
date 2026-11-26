# Hello World：可证明有限的变量边界循环

## 设计意图

两个独立的整数边界才可套用矩形次数证明。依赖外层计数器的三角形范围、写入计数器或执行额外循环体语句，不得被当成相同矩形；是否可由另一种已定义的证明归约，要另行判断。

嵌套循环若在仍可见的作用域重复声明同名局部变量，由源码绑定准入拒绝。矩形证明不再承担第二次 Java 名称合法性检查。

检验上限来自输入但源码仍给出推进方向和有限性的情形。零或负边界可零次执行；正边界保留与真实次数的关系，不能按任意样本展开。

## 输入原始代码

循环次数由入口参数决定，但每次只累加一个常量，边界和结果都可以由有限整数区间表达。

```java
class VariableLoop {
    static int count(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            total = total + 1;
        }
        return total;
    }
}
```

## AI 预处理结果

AI 只替换变量次数的循环片段，保留 Java 入口、初值和返回语句。区间是 `[0, limit)`，负边界对应空区间。

```diff
--- a/src/VariableLoop.java
+++ b/src/VariableLoop.java
@@ -2,8 +2,8 @@
     static int count(int limit) {
         int total = 0;
-        for (int i = 0; i < limit; i++) {
-            total = total + 1;
-        }
+        /* @normalizer-ir
+           total = count from [0, limit) as i
+        */
         return total;
     }
 }
```

**预处理要点**：AI 只闭合工程无法直接展开的循环片段。可证明的有限边界进入统一折叠语义，入口和外围 Java 仍由原生 parser 处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/VariableLoop.java#VariableLoop.count
  returns: number

P:
  parameters:
    limit: { position: 0, type: number }

R: |-
  CASE
    WHEN limit > 0 THEN limit
    ELSE 0
  END
```

**PERS 验收要点**：符号化执行只需要面对有限且可证明的迭代域，循环的执行过程可以归约为业务数值 CASE，同时保留负边界的零次语义。

包含上端点的对照将有效长度改为 `limit + 1`。仍按普通整数计数的统一取向用“长度 > 0”作为条件；`limit = -1` 返回 0，`limit = 0` 返回 1。
