# Hello World：自定义类型的可空字段

## 输入原始代码

`Profile` 本身是普通非空输入，只有 `label` 字段允许显式空值。入口根据字段是否为空返回标记值。

```java
import normalizer.annotations.Nullable;

class Profile {
    @Nullable
    String label;
}

class OptionalField {
    static int hasLabel(Profile profile) {
        return profile.label == null ? 0 : 1;
    }
}
```

## AI 预处理结果

**原样通过。** 可空性已经由 Normalizer 自身的 `@Nullable` 标记表达。AI 不把 `label` 提升成独立入口参数，也不把空值分支提前求值。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留 `Profile`、可空字段声明和原生三元表达式。 | 可空性属于类型合同，预处理只保留显式声明，字段读取、空值分支和最终异常域交给统一 IR。 |

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
    WHEN profile.label != null THEN 1
    ELSE 0
  END
```

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只有 Java 字段和 null 判断；new 在 `types.Profile` 中把 `label` 声明为 `text?`，并按 0/1 数值指示器规则，以 `profile.label != null` 表示返回 1 的分支。 | “普通对象非空”和“对象字段可空”是两层合同。空值只在显式可空字段的取得域内成立，不能由字段名或一次 null 比较反推整个对象可空。 |
