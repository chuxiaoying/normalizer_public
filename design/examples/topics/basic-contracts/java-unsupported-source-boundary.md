# Hello World：无法闭合的 JDK 调用必须拒绝

## 输入原始代码

入口直接调用 JDK 的 `Integer.toString`。调用的实现位于当前业务源码之外，源码没有提供可供 Normalizer 执行的函数体或外部合同。

```java
class UnsupportedBoundary {
    static String format(int value) {
        return Integer.toString(value);
    }
}
```

## AI 预处理结果

**拒绝。** AI 不把 JDK 内部实现抄进业务工程，也不把结果猜成固定文本。应补充可执行的业务函数体或明确的外部调用合同后再进入 Normalizer。

```text
PREPROCESS_REQUIRED

format:
  call: Integer.toString(value)
  reason: JDK implementation is outside the current source boundary
  required: a closed function body or an explicit external contract
```

## 归一化最终结果

未生成 PERS。CLI 必须保留 `PREPROCESS_REQUIRED` 诊断并指出 `Integer.toString/1`，不能用空实现、默认值或伪造输入继续归一化。

**预处理要点**：Out-of-source 是输入闭合问题，AI 应报告缺口；工程不按 JDK 方法名写特判，也不吞掉不可观察的计算。

**PERS 验收要点**：未证明的外部实现不能伪装成业务结果，拒绝本身也是可回归的边界合同。
