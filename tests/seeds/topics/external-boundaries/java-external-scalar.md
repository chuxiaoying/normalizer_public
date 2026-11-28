# Hello World：外部标量成功与失败

## 设计意图

区分实际合法端口的标量回执、失败以及普通同名入参调用。结果只能在对应成功取得域中使用；改接收者可能从合法 S 变成无源码 Panic。显式限定的 this 字段接收者不受同名局部标量遮蔽；把该局部量作为实参，只改变送入合法端口的参数表达式。

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

AI 在外部对象声明处增加 `@N.External` 并显式装配入口接收者。调用、try/catch 和返回路径保持 Java 原生结构。

```diff
--- a/src/RiskService.java
+++ b/src/RiskService.java
@@ -1,21 +1,28 @@
+import normalizer.intrinsics.N;
+
 class RiskService {
+    @N.External
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

-interface RiskClient {
-    int fetchScore(int accountId) throws RiskUnavailable;
+class RiskClient {
+    native int fetchScore(int accountId) throws RiskUnavailable;
 }

 class RiskUnavailable extends RuntimeException {}
+
+class Beans {
+    static final RiskService ENTRY = new RiskService(new RiskClient());
+}
```

Normalizer 工程只识别自己的 `@N.External` 标记。它不需要理解 `RiskClient` 来自哪个库，也不为 `fetchScore` 这个方法名增加特判。

**预处理要点**

- 特殊含义通过 Normalizer 标记进入统一 IR，工程不按外部库名称和方法名做特判。
- 标记应补充边界身份，源代码结构和语言语义继续由 Normalizer 按 Java 语义处理。

`Beans.ENTRY` 定位已装配实例；`native` 端口声明交付原签名，按[外部端口合同](../../../../design/chapters/04-AI救赎.md#ai-08-外部端口要有真实合同)处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/RiskService.java#Beans.ENTRY.riskScore
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

S:
  - call:
      external: src/RiskService.java#Beans.ENTRY.riskClient.fetchScore
      args:
        - accountId
      returns: number
      result: $e_0

R: |-
  CASE
    WHEN completion.is_return($e_0) THEN $e_0
    ELSE 0
  END
```

外部失败被入口内的 catch 消化，因此不形成未处理异常出口。成功返回的标量直接作为业务结果使用。

**PERS 验收要点**

- 外部边界需要记录调用事实和完成结果，PERS 才能表达成功与失败的业务关系。
- 外部标量成功返回值可以直接进入业务 PERS，避免为正常结果增加无意义的包装对象。

**同名接收者对照**　显式写 `this.riskClient` 仍调用已标注的字段。给入口增加同名普通形参后，裸 `riskClient.fetchScore(...)` 指向形参，不会继承字段的 external 合同。若该实参没有可归约的实际实现，则到内核时 Panic，不产出 S 或 PERS；改成 `this.riskClient` 才明确使用注入端口。

当入口仅在 `enabled` 时执行 `client.read(input) * 0`，原调用和它可能抛出的异常都受这个守卫保护。成功域的乘积化成 0，不等于删除调用；未发起调用的路径仍返回自己原有的值 1。把 0 放在乘号左边也不能跳过真正的调用和它的失败。
