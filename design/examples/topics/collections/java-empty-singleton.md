# Hello World：空集合与单元素首项

## 设计意图

区分空集合、单元素和多元素，检验索引零、非零索引及越界。等价的局部别名不改变首项；没有元素时不能默认凭空构造一个值。

已取得的非空借用数组，长度读取不读取元素、不产生索引异常；局部借用别名仍指向同一个入口数组。

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

**验收要点**：空集合、单元素和多元素是同一个首项合同的不同输入域，默认值和位置读取都必须保留。

| 切片起点 | 独立验收 |
|---|---|
| `1` | 长度至少为 2 时读第二个元素，否则返回 0 |
| `18446744073709551616`（2⁶⁴） | 与普通非负起点遵循同一规则：长度 `<= k` 返回默认值，否则读取 `values[k]`。比较和索引使用同一个精确的 `k`，不能因宿主整数位宽而报错、截断或回绕。这里的 `k` 是局部 IR 的业务索引，不是 Java 的定宽数组下标表达式。 |
| `-1` 或符号名 | 当前 `[k:]` 语法只接受非负整数字面量，入口拒绝；不把非法索引当成空集合，也不把符号名猜成常量。 |

## 直接读第零项不吞掉越界

```java
class FirstValue {
    static int firstRequired(int[] values) {
        return values[0];
    }
}
```

**原样通过。** 对已取得的有序数组，成功域读取第一项。空数组在原位置因越界失败，不是一个返回值为 null 的成功结果。R 表达同一个有序首项，异常在独立的终态域表达。

```yaml
schema: normalizer.pers/1
entry: { function: src/FirstValue.java#FirstValue.firstRequired, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: FIRST(value FROM values)
S: []
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'LENGTH(values) == 0' }
```
