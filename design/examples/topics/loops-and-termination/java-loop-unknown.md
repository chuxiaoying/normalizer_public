# Hello World：无法闭合的循环必须在预处理阶段拒绝

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
