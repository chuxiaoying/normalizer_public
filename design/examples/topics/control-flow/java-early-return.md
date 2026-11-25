# Hello World：早返回收束

## 输入原始代码

入口根据开关在 fallback 和主值之间选择，两个出口都正常完成。

```java
class Early {
    static int choose(boolean useFallback, int value, int fallback) {
        if (useFallback) return fallback;
        return value;
    }
}
```

## AI 预处理结果

**原样通过。** 早返回是当前 Java parser 支持的控制流，不需要入口级改写。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Early.java#Early.choose
  returns: number

P:
  parameters:
    useFallback:
      position: 0
      type: bool
    value:
      position: 1
      type: number
    fallback:
      position: 2
      type: number

R: CASE WHEN useFallback THEN fallback ELSE value END
```

**预处理要点**：工程能理解的有限控制流继续由工程归一化，AI 不把普通 Java 翻译成扩展语法。

**PERS 验收要点**：多出口正常路径统一为一个结果表达式，同时保留选择顺序。
