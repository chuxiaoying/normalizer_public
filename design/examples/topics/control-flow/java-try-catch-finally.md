# Hello World：finally 在正常和异常路径都产生状态写入

## 输入原始代码

外部读取成功时返回分数，声明的异常由 catch 转成零。finally 无论哪条路径完成都会把入口状态中的尝试次数加一。

```java
class RemoteError extends RuntimeException {}

class FinallyService {
    private final ScoreClient client;
    int attempts;

    FinallyService(ScoreClient client) { this.client = client; }

    int read(int accountId) {
        try {
            return client.fetch(accountId);
        } catch (RemoteError ignored) {
            return 0;
        } finally {
            attempts = attempts + 1;
        }
    }
}

interface ScoreClient {
    int fetch(int accountId) throws RemoteError;
}
```

## AI 预处理结果

AI 只在外部端口和已有状态声明处增加 Normalizer 自身的标记。try、catch、finally、两条返回路径和状态写入保持 Java 原生结构。

```diff
--- a/src/FinallyService.java
+++ b/src/FinallyService.java
@@ -1,1 +1,4 @@
+import normalizer.annotations.EntryState;
+import normalizer.annotations.External;
+
 class RemoteError extends RuntimeException {}
@@ -3,3 +6,5 @@
 class FinallyService {
+    @External
     private final ScoreClient client;
+    @EntryState
     int attempts;
```

**预处理要点**：预处理补充边界身份，不能把 finally 提前合并进正常返回，也不能把异常路径当成普通默认值。最终顺序由统一执行内核表达。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/FinallyService.java#FinallyService.read
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

E:
  attempts:
    kind: state
    source: src/FinallyService.java#FinallyService.attempts
    type: number

S:
  - call:
      external: client.fetch
      args:
        - accountId
      returns: number
      result: $e_0
  - write:
      target: attempts
      value: attempts + 1

R: |-
  CASE
    WHEN completion.is_throw($e_0, RemoteError) THEN 0
    ELSE $e_0
  END
```

**PERS 验收要点**：finally 是完成路径上的有序业务效果。它不能被吸收进 R，也不能因为异常被 catch 就从 S 中删除。

派生对照在 finally 写入后执行 `return 42`。无论外部调用正常完成还是被 catch，R 都是 42，S 仍按原顺序包含外部调用及一次状态写入。

若 finally 写入后无条件抛出 `RemoteError`，则它取代 try 和 catch 的所有正常完成。S 仍保留同一次外部调用及写入；`R: ⊥`，`throws` 在全域报告 `RemoteError`。
