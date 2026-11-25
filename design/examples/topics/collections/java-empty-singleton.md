# Hello World：空集合与单元素首项

## 输入原始代码

从有序数列读取第一项，空集合返回零。

```java
class FirstValue {
    static int firstOrZero(int[] values) {
        int result;
        /* @normalizer-ir
           result = first from values else 0
        */
        return result;
    }
}
```

## AI 预处理结果

**原样通过。** 局部 `first` 语义保留集合顺序和空集合默认值。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/FirstValue.java#FirstValue.firstOrZero, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: CASE WHEN LENGTH(values) == 0 THEN 0 ELSE values[0] END
```

**升级思想**　空集合、单元素和多元素是同一个首项合同的不同输入域，默认值和位置读取都必须保留。
