# Hello World：显式可空文本输入

## 输入原始代码

入口允许文本参数为空，并把这个可空性作为入口合同的一部分。入口返回是否为空的判断结果。

```java
class OptionalText {
    static boolean isMissing(@normalizer.annotations.Nullable String value) {
        return value == null;
    }
}
```

## AI 预处理结果

**原样通过。** 可空性已经由 Normalizer 自身标记显式声明，AI 不增加隐含的 null 分支。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/OptionalText.java#OptionalText.isMissing
  returns: bool

P:
  parameters:
    value:
      position: 0
      type: text?

R: value == null
```

**预处理要点**：可空性必须主动标记，不能从 Java 引用类型的宿主能力推测业务允许 null。

**PERS 验收要点**：普通类型默认非空，可空范围由统一类型合同显式传播到 PERS。
