# Hello World：无法闭合的 native 调用必须拒绝

## 设计意图

区分合法声明与实际不可分析的调用。源码不闭合时明确拒绝且无 PERS；没有真实外部边界证据，不能把未知实现标成端口。

## 输入原始代码

入口调用项目内声明的 native 方法。只知道它收一个整数并返回文本；没有方法体、标准操作定义或可验证的外部完成合同。

```java
class UnsupportedBoundary {
    static String format(int value) {
        return runtimeFormat(value);
    }

    static native String runtimeFormat(int value);
}
```

## AI 预处理结果

**拒绝。** 指出 `UnsupportedBoundary.runtimeFormat(int)` 缺少实现或外部合同；不复用看似同名的标准函数、不假设默认文本，也不把回执伪造为入口参数。

## 归一化最终结果

未生成 PERS。如果把原输入直接交给 Normalizer，执行到无定义调用时报告带源码位置的 `Panic: not-support`，而不输出一份普通业务异常或 S。这与「有可验证合同的真实外部端口」不同。
