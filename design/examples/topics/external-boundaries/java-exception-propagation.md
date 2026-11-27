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

AI 在外部对象声明处增加 `@N.External` 并显式装配入口接收者，调用和未捕获异常路径继续保持 Java 原生结构。

```diff
--- a/src/Pass.java
+++ b/src/Pass.java
@@ -1,17 +1,24 @@
+import normalizer.intrinsics.N;
+
 class Pass {
+    @N.External
     private final RiskClient client;

     Pass(RiskClient client) {
         this.client = client;
     }

     int read(int accountId) {
         return client.fetch(accountId);
     }
 }

-interface RiskClient {
-    int fetch(int accountId) throws RiskUnavailable;
+class RiskClient {
+    native int fetch(int accountId) throws RiskUnavailable;
 }

 class RiskUnavailable extends RuntimeException {}
+
+class Beans {
+    static final Pass ENTRY = new Pass(new RiskClient());
+}
```

Normalizer 识别 `@N.External` 和源码中的 `throws RiskUnavailable`，不按客户端库名做特判。

分析入口从 `Beans.ENTRY` 取得已装配实例。端口的 `native` 声明只交付原签名，内核按 `@N.External` 记录调用并绑定符号回执，不执行本机代码。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Pass.java#Beans.ENTRY.read
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

R: $e_0

S:
  - call:
      external: src/Pass.java#Beans.ENTRY.client.fetch
      args:
        - accountId
      returns: number
      result: $e_0

throws:
  - type: RiskUnavailable
    when: completion.is_throw($e_0, RiskUnavailable)
```

**预处理要点**

- 外部身份由 Normalizer 自身标记提供，不需要执行具体库。
- 预处理补边界身份，但不吞掉异常或代替工程推导完成路径。

**PERS 验收要点**：外部调用的成功值、失败类型和入口完成关系必须同时可观察。

对照同一回执上的正常读取、指定异常载荷读取和异常判断。三者共享回执身份，但各自保留返回类型和必要参数；不是同一种结构化读取。此断言不把部分投影移动到它所属路径之外。
