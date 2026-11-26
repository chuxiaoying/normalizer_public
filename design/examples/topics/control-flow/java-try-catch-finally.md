# Hello World：finally 在正常和异常路径都产生状态写入

## 设计意图

检验正常返回、已捕获和未捕获异常都按真实顺序进入 finally。finally 写入不能丢失；异常类型相同、父类匹配或无关处理器必须按各自可达条件区分。没有匹配的外部异常在 finally 正常结束后继续传播；若 finally 在写入后抛出另一种已声明的无载荷异常，则只报告后者。连续两次写入须按顺序保留，第二次读取第一次写后的状态。

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

**只按类型匹配的对照**　外部端口仍声明抛出 `RemoteError`，但增加 `ParentError`，让继承链成为 `RemoteError → ParentError → RuntimeException`。先放一个不匹配的同级 `OtherError` handler，再捕获 `RuntimeException`。应穿过中间父类命中后者，不会返回第一个 handler 的 999。回执里仍是实际的 `RemoteError`，finally 仍仅写一次。此例不读取或构造异常对象的载荷。继承有环或根不在当前 RuntimeException 合同内时，必须在源码合同校验时拒绝。

若 finally 写入后无条件抛出 `RemoteError`，则它取代 try 和 catch 的所有正常完成。S 仍保留同一次外部调用及写入；`R: ⊥`，`throws` 在全域报告 `RemoteError`。

若 finally 先写入，再仅对 `accountId == 0` 返回 42，它只覆盖该域上的正常完成以及异常转零。其他账户仍按原 try/catch 返回；同一次调用和一次状态写入留在 S 中。

### 异常处理器的类型准入与匹配

先判断 catch 声明能不能作为当前源码合同里的异常类型，再判断它会不会命中。一个合法但不匹配的处理器不执行；不合法的声明不是“不匹配”，必须由入口拒绝，不能进入模拟执行或生成 PERS。

| 变化 | 可独立判定的结果 |
| --- | --- |
| 使用完全限定名 `java.lang.RuntimeException` 捕获声明的 `RemoteError` | 与简名捕获父类一致；仍发布原调用、一次 finally 写入和原先的两条结果路径 |
| 捕获 `final RemoteError ignored` | `final` 只约束局部绑定，不影响处理器匹配、回执或 finally |
| 处理器写成原始类型 `int` 或异常类数组 `RemoteError[]` | 均不是可捕获的异常类，拒绝，不将数组元素类型当作所捕获的类型 |
| 写成未声明的 `MissingError`，或指向已声明但未继承异常根的 `Ordinary` | 均不能闭合异常身份，拒绝；普通类出现在源码中并不赋予它异常语义 |

捕获类型即使出现在未被调用的方法里，或写在已经匹配的处理器之后，也必须在入口核验。后面的处理器若重复捕获同一个类型（包括简名与限定名的不同写法），或其类型已经被前面的父类处理器覆盖，源码不能通过编译，入口拒绝。反过来，先捕获子类再放父类是合法的；实际收到的子类异常仍进入第一个处理器。

这些对照不扩大到任意 Java 异常对象或嵌套处理器的执行范围。 按[已发布的源码合同](../../../../implementation/contracts/normalized-source-contract.md)，finally 末尾显式抛出目前只接收源码声明的无参、无载荷异常；带字符串载荷的 RuntimeException 是合法 Java，但不属于该终端 finally 子集，不能退回按一般 throw 执行。
