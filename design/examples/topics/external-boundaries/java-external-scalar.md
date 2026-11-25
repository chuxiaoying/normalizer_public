# Hello World：外部标量成功与失败

## 输入原始代码

服务读取外部风险分数。读取成功时返回外部分数，外部服务抛出声明的不可用异常时返回 0。

```java
class RiskService {
    private final RiskClient riskClient;

    RiskService(RiskClient riskClient) {
        this.riskClient = riskClient;
    }

    int riskScore(int accountId) {
        try {
            return riskClient.fetchScore(accountId);
        } catch (RiskUnavailable ignored) {
            return 0;
        }
    }
}

interface RiskClient {
    int fetchScore(int accountId) throws RiskUnavailable;
}

class RiskUnavailable extends RuntimeException {}
```

## AI 预处理结果

AI 只在外部对象声明处增加 Normalizer 自身的标记。调用、try/catch 和返回路径保持 Java 原生结构。

```diff
--- a/src/RiskService.java
+++ b/src/RiskService.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.External;
+
 class RiskService {
+    @External
     private final RiskClient riskClient;
```

Normalizer 工程只识别自己的 `@External` 标记。它不需要理解 `RiskClient` 来自哪个库，也不为 `fetchScore` 这个方法名增加特判。

**预处理要点**

- 特殊含义通过 Normalizer 标记进入统一 IR，工程不按外部库名称和方法名做特判。
- 标记应补充边界身份，源代码结构和语言语义继续由 Java parser 处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/RiskService.java#RiskService.riskScore
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

S:
  - call:
      external: riskClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_0

R: |-
  CASE
    WHEN completion.is_throw($e_0, RiskUnavailable) THEN 0
    ELSE $e_0
  END
```

外部失败被入口内的 catch 消化，因此不形成未处理异常出口。成功返回的标量直接作为业务结果使用。

**PERS 验收要点**

- 外部边界需要记录调用事实和完成结果，PERS 才能表达成功与失败的业务关系。
- 外部标量成功返回值可以直接进入业务 PERS，避免为正常结果增加无意义的包装对象。
