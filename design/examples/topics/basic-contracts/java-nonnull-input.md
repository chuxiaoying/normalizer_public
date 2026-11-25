# Hello World：普通输入默认非空

## 输入原始代码

入口接收两个普通文本参数并返回主文本。业务合同没有允许空值的声明，案例重点是普通参数的统一非空类型合同。

```java
class Greeting {
    static String label(String name, String fallback) {
        return name;
    }
}
```

## AI 预处理结果

**原样通过。** 这是 Java 原生参数返回。Normalizer 按统一业务类型合同把未标记的普通参数视为非空，不为宿主语言允许的 null 自动增加异常路径。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保持原始 Java。 | 普通输入合同由统一业务类型规则提供，AI 和工程都不为每个案例临时添加空值假设。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Greeting.java#Greeting.label
  returns: text

P:
  parameters:
    name:
      position: 0
      type: text
    fallback:
      position: 1
      type: text

R: name
```

如果业务允许 name 为空，源码或预处理合同必须显式写出 `text?` 和对应处理。当前案例保持两个普通非空输入，因此没有空引用异常分支。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 需要从 Java 的可空引用能力推测是否存在 null 分支，new 在 P 中以 `text` 交代两个普通文本输入非空。 | 输入有效性属于统一业务合同，不能由每个入口依赖宿主语言的默认能力自行猜测。 |
| old 可能额外生成 `name == null` 的异常路径，new 只有主文本返回表达式。 | 没有显式可空声明时，PERS 专注于业务允许的输入域。 |
