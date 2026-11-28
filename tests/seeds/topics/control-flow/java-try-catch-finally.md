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

规范源码显式装配原接收者，在其外部端口和已有状态声明处增加 Normalizer 标记。端口保留签名与声明异常，不在本地伪造返回。try、catch、finally、两条返回路径和状态写入保持 Java 原生结构。

```diff
--- a/src/FinallyService.java
+++ b/src/FinallyService.java
@@ -1,22 +1,28 @@
+import normalizer.intrinsics.N;
+
 class RemoteError extends RuntimeException {}

 class FinallyService {
+    static final FinallyService INSTANCE = new FinallyService(new ScoreClient());
+
+    @N.External
     private final ScoreClient client;
+    @N.EntryState
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

-interface ScoreClient {
-    int fetch(int accountId) throws RemoteError;
+class ScoreClient {
+    native int fetch(int accountId) throws RemoteError;
 }
```

**预处理要点**：预处理补充边界身份，不能把 finally 提前合并进正常返回，也不能把异常路径当成普通默认值。最终顺序由统一执行内核表达。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/FinallyService.java#FinallyService.INSTANCE.read
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

E:
  attempts:
    kind: state
    source: src/FinallyService.java#FinallyService.INSTANCE.attempts
    type: number

S:
  - call:
      external: src/FinallyService.java#FinallyService.INSTANCE.client.fetch
      args:
        - accountId
      returns: number
      result: $e_0
  - write:
      target: attempts
      value: attempts + 1

R: |-
  CASE
    WHEN completion.is_return($e_0) THEN $e_0
    ELSE 0
  END
```

**PERS 验收要点**：finally 是完成路径上的有序业务效果。它不能被吸收进 R，也不能因为异常被 catch 就从 S 中删除。

派生对照在 finally 写入后执行 `return 42`。无论外部调用正常完成还是被 catch，R 都是 42，S 仍按原顺序包含外部调用及一次状态写入。

**只按类型匹配的对照**　外部端口仍声明抛出 `RemoteError`，但增加 `ParentError`，让继承链成为 `RemoteError → ParentError → RuntimeException`。先放一个不匹配的同级 `OtherError` handler，再捕获 `RuntimeException`。应穿过中间父类命中后者，不会返回第一个 handler 的 999。回执里仍是实际的 `RemoteError`，finally 仍仅写一次。此例不读取或构造异常对象的载荷。继承有环或根不在当前 RuntimeException 合同内时，必须在源码合同校验时拒绝。

若 finally 写入后无条件抛出 `RemoteError`，则它取代 try 和 catch 的所有正常完成。S 仍保留同一次外部调用及写入；没有正常完成，因此不写 `R`，`throws` 在全域报告 `RemoteError`。

若 finally 先写入，再仅对 `accountId == 0` 返回 42，它只覆盖该域上的正常完成以及异常转零。其他账户仍按原 try/catch 返回；同一次调用和一次状态写入留在 S 中。

### 异常处理器的类型准入与匹配

先判断 catch 声明能不能作为当前源码合同里的异常类型，再判断它会不会命中。一个合法但不匹配的处理器不执行；不合法的声明不是“不匹配”，必须由入口拒绝，不能进入模拟执行或生成 PERS。

| 变化 | 可独立判定的结果 |
| --- | --- |
| 使用完全限定名 `java.lang.RuntimeException` 捕获声明的 `RemoteError` | 与简名捕获父类一致；仍发布原调用、一次 finally 写入和原先的两条结果路径 |
| 捕获 `final RemoteError ignored` | `final` 只约束局部绑定，不影响处理器匹配、回执或 finally |
| 处理器写成原始类型 `int` 或异常类数组 `RemoteError[]` | 均不是可捕获的异常类，拒绝，不将数组元素类型当作所捕获的类型 |
| 写成未声明的 `MissingError`，或指向已声明但未继承异常根的 `Ordinary` | 均不能闭合异常身份，拒绝；普通类出现在源码中并不赋予它异常语义 |

捕获类型即使出现在未被调用的方法里，或写在已经匹配的处理器之后，也必须在入口核验。后面的处理器若重复捕获同一个类型（包括简名与限定名的不同写法），或其类型已经被前面的父类处理器覆盖，源码不能通过编译，入口拒绝。反过来，先捕获子类再放父类是合法的。若 `catch (RemoteError e)` 返回 11、后续 `catch (RuntimeException e)` 返回 22，而该端口只声明 `RemoteError`，失败一定走第一个处理器；22 不进入规范结果，finally 仍写一次。两个匹配的类型有包含关系，不等于两种互斥的完成。

这些对照不要求假定未声明的异常域或载荷。若 finally 实际抛出有载荷的异常，类型、发生条件和可观察载荷都属于该终态；不能为了套用无载荷示例而丢掉文本，或回退到被覆盖的原异常。

若 finally 中的返回守卫恒假，它不会覆盖任何待完成结果；原调用和一次写入不变。若 finally 按账户条件在两个分支都显式返回，它则覆盖所有原完成结果，R 只取决于该条件，原调用、写入顺序及次数仍保持。

### 可证的资源关闭

把原 try 的关闭动作交给源码中的 `java.lang.AutoCloseable`，资源析构必须在外层 finally 之前发生，两处写入不合并。资源用闭合构造器准确绑定 owner 和 increment 两个 final 字段时，关闭使用绑定到实际实例的 increment。合法的、可能抛出的关闭不属于这个只证明正常关闭的子集；尤其在 try 已经失败时，不能丢掉关闭的异常，也不能当成第二个互不相关的正常结果。

`finally` 中的无参抛出和普通语句一样按已声明的异常类型执行。把末尾的 `throw new RemoteError()` 换成 `throw new java.lang.ArithmeticException()`，最终只报告该原生异常的完整类型名；之前的调用和写入不变。若改为仅当 `accountId == 0` 才抛出 `RemoteError`，只在该条件下覆盖待完成值，其余路径保留原 try/catch 的结果。若构造的是普通类 `Ordinary` 而非异常类，则入口拒绝，不发布 PERS。
