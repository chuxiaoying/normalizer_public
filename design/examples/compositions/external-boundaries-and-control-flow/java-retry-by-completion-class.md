# 仅在匹配的失败类型上重试

## 设计意图

同一端口声明两种真实失败，只有捕获类型的失败授权下一次调用。这个联合场景不同于[单一可重试异常](../../topics/loops-and-termination/java-loop-bounded-retry.md)：失败既不能一概重试，也不能一概走后备值；未调用时没有第二个回执。

## 输入原始代码

调用方已装配真实报价服务。服务最多尝试两次；暂时错误允许下一轮，终止错误直接传播。

```java
class QuoteRetry {
    private final QuotePort port;

    QuoteRetry(QuotePort port) {
        this.port = port;
    }

    int quote(int accountId, int fallback) {
        for (int attempt = 0; attempt < 2; attempt++) {
            try {
                return port.quote(accountId);
            } catch (TransientQuote ignored) {
                // Only this completion permits retry.
            }
        }
        return fallback;
    }
}

interface QuotePort {
    int quote(int accountId) throws TransientQuote, TerminalQuote;
}

class TransientQuote extends RuntimeException {}
class TerminalQuote extends RuntimeException {}
```

## AI 预处理结果

标记真实边界，并交付与调用方装配一致的静态入口定位。保留原生循环、捕获类型和调用位置，不展开循环，不追加后备调用。

```diff
--- a/src/QuoteRetry.java
+++ b/src/QuoteRetry.java
@@ -1,25 +1,30 @@
+import normalizer.intrinsics.N;
+
 class QuoteRetry {
+    static final QuoteRetry INSTANCE = new QuoteRetry(new QuotePort());
+
+    @N.External
     private final QuotePort port;

     QuoteRetry(QuotePort port) {
         this.port = port;
     }

     int quote(int accountId, int fallback) {
         for (int attempt = 0; attempt < 2; attempt++) {
             try {
                 return port.quote(accountId);
             } catch (TransientQuote ignored) {
                 // Only this completion permits retry.
             }
         }
         return fallback;
     }
 }

-interface QuotePort {
-    int quote(int accountId) throws TransientQuote, TerminalQuote;
+class QuotePort {
+    native int quote(int accountId) throws TransientQuote, TerminalQuote;
 }

 class TransientQuote extends RuntimeException {}
 class TerminalQuote extends RuntimeException {}
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/QuoteRetry.java#QuoteRetry.INSTANCE.quote, returns: number}
P:
  parameters:
    accountId: {position: 0, type: number}
    fallback: {position: 1, type: number}
S:
  - call:
      external: src/QuoteRetry.java#QuoteRetry.INSTANCE.port.quote
      args: [accountId]
      returns: number
      result: $e_0
  - call:
      external: src/QuoteRetry.java#QuoteRetry.INSTANCE.port.quote
      args: [accountId]
      returns: number
      result: $e_1
      when: completion.is_throw($e_0, TransientQuote)
R: |-
  CASE
    WHEN completion.is_return($e_0) THEN $e_0
    WHEN completion.is_return($e_1) THEN $e_1
    ELSE fallback
  END
throws:
  - type: TerminalQuote
    when: |-
      CASE
        WHEN completion.is_throw($e_0, TransientQuote) THEN completion.is_throw($e_1, TerminalQuote)
        ELSE completion.is_throw($e_0, TerminalQuote)
      END
```

| 首次完成 | 第二次完成 | 最终观察 |
|---|---|---|
| 成功（包括零或负数） | 未发起 | 只记第一次调用，返回它的正常值。 |
| 终止错误 | 未发起 | 只记第一次调用；抛出终止错误，无正常结果。 |
| 暂时错误 | 成功 | 两次调用，返回第二次的正常值。 |
| 暂时错误 | 终止错误 | 两次调用；抛出第二次的终止错误，无正常结果。 |
| 暂时错误 | 暂时错误 | 两次调用，返回入口的后备值。 |

同一次调用只能占一个完成标签，所以第二次调用条件不额外检查首次“不是终止错误”。顶层异常条件先划分第二次回执是否存在：发生重试才检查第二个标签，否则只检查第一个。这不是可以任意重排的 OR。正常出口只在非抛出路径求值；首个正常分支未命中且仍处于正常出口时，首次必为暂时错误，因此后面的分支能读已取得的第二次回执。

把上限改成一次必须彻底消除第二次调用及回执。若改捕获终止错误，两种失败的角色互换：原可重试类转为立即传播，原终止类才会触发第二次调用。两次收到相同参数也仍是独立观察，不能合并回执。
