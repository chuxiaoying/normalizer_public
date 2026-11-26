# Hello World：显式可空文本输入

## 设计意图

对照 null、空串和非空文本，检验显式可空的入口合同和零风险的 null 判断。反转测试只改变谓词真假，不能把 null 检查解释为调用或把可空类型改成普通类型。

参数显式写为 java.lang.String 不改变 String 类型的可空性、参数位置或比较结果。

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

**PERS 验收要点**：普通类型默认非空，可空范围由统一类型合同显式传播到 PERS。同一段空值判断也可放在有源码的 `String` helper 内，由入口传入字面量 `null`。该次 helper 的参数值明确为空，结果恒为真；不要新增外部调用，也不要把已有入口参数假装成这次 helper 的实参。
