# Hello World：固定上限重试与运行时次数的积分循环

## 设计意图

对照固定上限重试和运行时次数聚合。首轮成功、失败后成功和达到上限必须产生不同的实际调用轨迹；已知次数可静态归约，但不得假定外部回执相同。

## 输入原始代码

评分服务最多尝试三次。若全部失败，每个可用积分抵一分。两个循环出现在同一个入口：重试的最大次数在源码中确定，积分循环的次数取决于参数。

```java
class ScoreService {
    private final ScoreClient scoreClient;

    ScoreService(ScoreClient scoreClient) {
        this.scoreClient = scoreClient;
    }

    int fetchScore(int accountId, int availableCredits) {
        for (int attempt = 0; attempt < 3; attempt++) {
            try {
                return scoreClient.fetchScore(accountId);
            } catch (RetryableScoreError ignored) {
                // retry
            }
        }
        int fallbackScore = 0;
        for (int credit = 0; credit < availableCredits; credit++) {
            fallbackScore++;
        }
        return fallbackScore;
    }
}

interface ScoreClient {
    int fetchScore(int accountId) throws RetryableScoreError;
}

class RetryableScoreError extends RuntimeException {}
```

评分成功立即返回，包括返回零或负数。只有三次均抛出可重试异常才进入第二个循环；积分为零或负数时，循环执行零次。积分按整数计数；业务 `number` 消除固定大小上限，不把整数积分扩张成任意小数。

## AI 预处理结果

预处理显式装配接收者、在端口声明处标记身份，在第二个循环原处补入计数 IR。第一个循环保留原 Java，包括提前返回和异常捕获。

```diff
--- a/src/ScoreService.java
+++ b/src/ScoreService.java
@@ -1,28 +1,33 @@
+import normalizer.intrinsics.N;
+
 class ScoreService {
+    static final ScoreService INSTANCE = new ScoreService(new ScoreClient());
+
+    @N.External
     private final ScoreClient scoreClient;

     ScoreService(ScoreClient scoreClient) {
         this.scoreClient = scoreClient;
     }

     int fetchScore(int accountId, int availableCredits) {
         for (int attempt = 0; attempt < 3; attempt++) {
             try {
                 return scoreClient.fetchScore(accountId);
             } catch (RetryableScoreError ignored) {
                 // retry
             }
         }
         int fallbackScore = 0;
-        for (int credit = 0; credit < availableCredits; credit++) {
-            fallbackScore++;
-        }
+        /* @normalizer-ir
+           fallbackScore += count from [0, availableCredits) as credit
+        */
         return fallbackScore;
     }
 }

-interface ScoreClient {
-    int fetchScore(int accountId) throws RetryableScoreError;
+class ScoreClient {
+    native int fetchScore(int accountId) throws RetryableScoreError;
 }

 class RetryableScoreError extends RuntimeException {}
```

Normalizer 可按固定上限分析最多三次尝试，无需 AI 复制循环体。第二段 IR 对整数区间 `[0, availableCredits)` 计数，表示原循环依次访问的所有积分编号。上界小于或等于零时区间为空。计数与初值的合并、失败分支的返回值均由内核推导。

**预处理要点**：同一入口内也要按循环性质分工：静态上限明确的控制流交给工程，次数取决于输入的计数只补充局部语义。AI 不应为处理后一段循环而接管前一段执行过程。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/ScoreService.java#ScoreService.INSTANCE.fetchScore
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number
    availableCredits:
      position: 1
      type: number

S:
  - call:
      external: src/ScoreService.java#ScoreService.INSTANCE.scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_0

  - call:
      external: src/ScoreService.java#ScoreService.INSTANCE.scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_1
      when: completion.is_throw($e_0, RetryableScoreError)

  - call:
      external: src/ScoreService.java#ScoreService.INSTANCE.scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_2
      when: |-
        CASE
          WHEN completion.is_return($e_0) THEN false
          ELSE completion.is_throw($e_1, RetryableScoreError)
        END

R: |-
  CASE
    WHEN completion.is_return($e_0) THEN $e_0
    WHEN completion.is_return($e_1) THEN $e_1
    WHEN completion.is_return($e_2) THEN $e_2
    WHEN availableCredits > 0 THEN availableCredits
    ELSE 0
  END
```

第 3 次取号只有前两次均以已声明异常完成才发生。若第 1 次成功，第 2 次回执未取得，判断不得读取它；这里各次调用只可能正常返回或抛声明的可重试异常；R 按首个命中执行，前一回执未正常返回才检查下一臂。无论读正常值还是完成判定，均不得先访问未取得的回执。第 4 次对照同样逐层检查已取得的回执。

**PERS 验收要点**：把重试过程收束为完成条件与结果的关系，消除重复控制流痕迹，同时保留每次调用的独立性和先后依赖。

入口、P 和数值域的共性见[基础案例](../expression-simplification/java-static.md)；条件归约的共性见[表达式化简](../expression-simplification/java-expression-simplification.md)。

若第一次只在 `Transient` 时再试一次，而两次都可能独立抛 `Terminal`，终态不能只写成「两次中任一次抛 Terminal」。第二次仅在第一次为 `Transient` 时已发出，故这个守卫先选择读第二次的终态，其他路径才读第一次的终态。第一次的 `Transient` 已被捕获；第二次若抛该异常才继续传播。不能把两次同名异常当作同一个回执。
