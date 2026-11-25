# Hello World：固定上限重试与运行时次数的积分循环

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

预处理只在端口声明处标记身份，在第二个循环原处补入计数 IR。第一个循环保留原 Java，包括提前返回和异常捕获。

```diff
--- a/src/ScoreService.java
+++ b/src/ScoreService.java
@@ -1,4 +1,7 @@
+import normalizer.annotations.External;
+
 class ScoreService {
+    @External
     private final ScoreClient scoreClient;

     ScoreService(ScoreClient scoreClient) {
@@ -14,9 +17,9 @@
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
```

Normalizer 可按固定上限分析最多三次尝试，无需 AI 复制循环体。第二段 IR 对整数区间 `[0, availableCredits)` 计数，表示原循环依次访问的所有积分编号。上界小于或等于零时区间为空。计数与初值的合并、失败分支的返回值均由内核推导。

**预处理要点**：同一入口内也要按循环性质分工：静态上限明确的控制流交给工程，次数取决于输入的计数只补充局部语义。AI 不应为处理后一段循环而接管前一段执行过程。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/ScoreService.java#ScoreService.fetchScore
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
      external: scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_0

  - call:
      external: scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_1
      when: completion.is_throw($e_0, RetryableScoreError)

  - call:
      external: scoreClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_2
      when: completion.is_throw($e_0, RetryableScoreError) and completion.is_throw($e_1, RetryableScoreError)

R: |-
  CASE
    WHEN completion.is_return($e_0) THEN $e_0
    WHEN completion.is_return($e_1) THEN $e_1
    WHEN completion.is_return($e_2) THEN $e_2
    WHEN availableCredits > 0 THEN availableCredits
    ELSE 0
  END
```

**PERS 验收要点**：把重试过程收束为完成条件与结果的关系，消除重复控制流痕迹，同时保留每次调用的独立性和先后依赖。

入口、P 和数值域的共性见[基础案例](java-static.md)；条件归约的共性见[表达式化简](java-expression-simplification.md)。
