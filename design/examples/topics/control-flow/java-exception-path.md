# Hello World：异常路径未闭合时拒绝归一化

## 输入原始代码

入口在负数输入时显式抛出异常，并在入口内捕获后返回 fallback。正常路径和异常路径都是业务行为。

```java
class Guarded {
    static int read(int value) {
        try {
            if (value < 0) throw new IllegalArgumentException();
            return value;
        } catch (IllegalArgumentException ignored) {
            return 0;
        }
    }
}
```

## AI 预处理结果

**拒绝。** 当前规范输入不能闭合这条原生 `throw/catch` 路径。AI 不把异常改成普通返回值，也不删除 catch 来换取可解析输入。

```text
PREPROCESS_REQUIRED

read:
  path: value < 0 -> IllegalArgumentException -> catch fallback
  required: an explicit exception-path contract accepted by the parser
```

## 归一化最终结果

未生成 PERS。CLI 保留非零退出和 `PREPROCESS_REQUIRED` 诊断。

**预处理要点**：异常路径是可观察业务行为，AI 不能用看似等价的条件返回代替它。

**PERS 验收要点**：未闭合的正常／异常路径不能被伪造成单一 R，缺口应进入可回归的准入边界。
