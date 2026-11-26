# Hello World：无法闭合的循环必须在预处理阶段拒绝

## 设计意图

区分源码可证明终止和仅凭运行时猜测终止。未知循环不能取一次展开或给出部分 PERS；补足真实可验证边界才进入有限循环合同。

若源码声明了这个普通接口的一种本地实现，不能仅凭接口形状把循环诊断为外部响应轮询，也不能把该实现当作运行时接收者。循环仍无已证明终止界，由普通循环入口报告能力缺口，不生成 PERS。

计算后再传入的轮询参数，或循环体中混入普通辅助调用，都不会凭空提供终止证明，也不应被当成已识别的纯外部轮询。普通循环入口报告能力缺口。

## 输入原始代码

本例的入口 `readWhenReady` 会轮询外部就绪状态，并在两次查询之间等待。

```java
class ReadinessService {
    private final ReadinessClient readinessClient;

    ReadinessService(ReadinessClient readinessClient) {
        this.readinessClient = readinessClient;
    }

    int readWhenReady(int accountId) {
        while (!readinessClient.isReady(accountId)) {
            readinessClient.waitForChange(accountId);
        }
        return readinessClient.readScore(accountId);
    }

    int recursiveNormalize(int value) {
        if (value <= 0) {
            return 0;
        }
        return recursiveNormalize(value - 1) + 1;
    }
}

interface ReadinessClient {
    boolean isReady(int accountId);
    void waitForChange(int accountId);
    int readScore(int accountId);
}
```

## AI 预处理结果

**拒绝。** 就绪状态取决于外部响应。源码没有给出等待次数上限或等价的完成合同；提前结束会改变外部调用次数及读取评分的时机。

```text
PREPROCESS_REQUIRED

readWhenReady:
  source loop: while (!readinessClient.isReady(accountId))
  termination: depends on external state and has no finite bound
  required: explicit readiness completion contract or bounded polling policy
```

同一类中的 `recursiveNormalize` 无法从 `readWhenReady` 到达，不参与此次入口分析。

## 归一化最终结果

未生成 PERS。

轮询条件写成 `!ready(...)`、`ready(...) == false` 或 `false == ready(...)`，都只依赖一次外部探测。比较项位置不提供有限次数保证，入口都要求明确终止合同，不生成 PERS。

显式写 `this.readinessClient` 仍指同一实例字段；它不从同名局部变量推断，也不把任意对象的字段调用扩成外部轮询识别。
