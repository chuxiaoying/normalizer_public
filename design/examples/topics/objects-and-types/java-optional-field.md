# Hello World：自定义类型的可空字段

## 设计意图

检验对象本身可用与它的某个字段可空是独立事实。字段 null、空文本和非空分别观察；反向分支可保持同一判定，不能把整对象都改成可空。

同一 JDK 类型 String 的简单名和限定名 java.lang.String 不改变已有可空字段声明、分支或 PERS 类型。

新建且未赋值的可空字段仍为 null，不增加入口参数或副作用。未标记的普通静态字段不能并入实例字段合同；源码入口报告预处理能力缺口，不把共享状态伪装成实例字段。

最终返回新建对象的可空字段时，返回记录应包含该字段。初始值 null 和明确写入的空字符串是不同内容；局部对象的字段初始化不产生 S。

## 输入原始代码

`Profile` 本身是普通非空输入，只有 `label` 字段允许显式空值。入口根据字段是否为空返回标记值。

```java
import normalizer.intrinsics.N;

class Profile {
    @N.Nullable
    String label;
}

class OptionalField {
    static int hasLabel(Profile profile) {
        return profile.label == null ? 0 : 1;
    }
}
```

## AI 预处理结果

**原样通过。** 可空性已经由 Normalizer 自身的 `@N.Nullable` 标记表达。AI 不把 `label` 提升成独立入口参数，也不把空值分支提前求值。

**预处理要点**：可空性属于类型合同，预处理只保留显式声明，字段读取、空值分支和最终异常域交给统一 IR。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      label: text?

entry:
  function: src/OptionalField.java#OptionalField.hasLabel
  returns: number

P:
  parameters:
    profile:
      position: 0
      type: Profile

R: |-
  CASE
    WHEN profile.label == null THEN 0
    ELSE 1
  END
```

**PERS 验收要点**：“普通对象非空”和“对象字段可空”是两层合同。空值只在显式可空字段的取得域内成立，不能由字段名或一次 null 比较反推整个对象可空。

另一个局部构造的对照给 `enabled` 初始化为 true、`byte debit` 初始化为 -128、`short credit` 初始化为 32767，仍不为可空 `label` 赋值。新对象中的布尔条件成立、label 仍为 null，选中数值之和 32639；字段都按实际初始化取得，不产生 E、S 或异常，也不把局部字段变成 P。
