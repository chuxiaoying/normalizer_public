# Hello World：外部异常向入口传播

## 设计意图

区分进入入口后的正常回执、外部失败与传播出去的异常。传播不许变成普通 R 或虚构成功；改捕获范围才会改变最终 throws。

## 输入原始代码

入口直接返回风险客户端结果。外部服务失败时，入口不捕获异常而向调用者传播。

```java
class Pass {
    private final RiskClient client;

    Pass(RiskClient client) {
        this.client = client;
    }

    int read(int accountId) {
        return client.fetch(accountId);
    }
}

interface RiskClient {
    int fetch(int accountId) throws RiskUnavailable;
}

class RiskUnavailable extends RuntimeException {}
```

## AI 预处理结果

AI 只在外部对象声明处增加 Normalizer 自身的标记，调用和未捕获异常路径继续保持 Java 原生结构。

```diff
--- a/src/Pass.java
+++ b/src/Pass.java
@@ -1,2 +1,5 @@
+import normalizer.annotations.External;
+
 class Pass {
+    @External
     private final RiskClient client;
```

Normalizer 识别 `@External` 和源码中的 `throws RiskUnavailable`，不按客户端库名做特判。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Pass.java#Pass.read
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

R: $e_0

S:
  - call:
      external: client.fetch
      args:
        - accountId
      returns: number
      result: $e_0

throws:
  - type: RiskUnavailable
    when: completion.is_throw($e_0, RiskUnavailable)
```

**预处理要点**

- 外部身份由 Normalizer 自身标记提供，Java parser 不需要理解具体库。
- 预处理补边界身份，但不吞掉异常或代替工程推导完成路径。

**PERS 验收要点**：外部调用的成功值、失败类型和入口完成关系必须同时可观察。
