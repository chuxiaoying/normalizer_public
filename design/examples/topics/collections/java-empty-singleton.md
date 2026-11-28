# Hello World：空集合与单元素首项

## 设计意图

同样已绑定、非空的 `long[]` 和 `short[]` 也按业务整数数组读长度：`length` 观察的是元素个数，不是元素的 Java 载体，不发起外部调用，也不读某个索引。

区分空集合、单元素和多元素，检验索引零、非零索引及越界。等价的局部别名不改变首项；没有元素时不能默认凭空构造一个值。

已取得的非空借用数组，长度读取不读取元素、不产生索引异常；局部借用别名仍指向同一个入口数组。 数组前面有其他参数时，长度仍绑定到原数组名；普通对象中恰好名为 `length` 的字段则按字段读取，不生成数组基数表达式。两种读法都不产生额外副作用。

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
R: CASE WHEN 0 == LENGTH(values) THEN 0 ELSE FIRST(value FROM values) END
```

**验收要点**：空集合、单元素和多元素是同一个首项合同的不同输入域，默认值和位置读取都必须保留。

| 切片起点 | 独立验收 |
|---|---|
| `1` | `CASE WHEN 2 > LENGTH(values) THEN 0 ELSE values[1] END`；仍是位置读取，不是首项。两个方向同成本，操作数定向后默认域使用 `>`，排在读项域的 `<` 前。 |
| `18446744073709551616`（2⁶⁴） | 与普通非负起点遵循同一规则：长度不超过 `k` 时返回默认值，否则读取相应项。完整结果为 `CASE WHEN 18446744073709551617 > LENGTH(values) THEN 0 ELSE values[18446744073709551616] END`；比较边界为精确的 `k + 1`，读项仍用原起点 `k`。比较和索引都不能因宿主整数位宽而报错、截断或回绕。这里的 `k` 是局部 IR 的业务索引，不是 Java 的定宽数组下标表达式。 |
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
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: '0 == LENGTH(values)' }
```

长度也可在源码可见的辅助函数中观察。把已绑定的 `values` 传给以 `items` 为形参名的长度函数，返回 `LENGTH(values)`；辅助函数形参名不能覆盖借用来源，也不能凭空产生数组读写、越界或 S。

## 首项参与比较，越界仍先发生

先把 `values[0]` 读入局部，再返回它和 `threshold` 中较大的值。空数组连比较都不能执行，仍在第一次读取处抛出越界，不能返回 threshold。单元素分别为 2、3、4 且 threshold 为 3 时，正常结果依次为 3、3、4。两值相等时两臂都是同一个数，可统一为 `CASE WHEN threshold > FIRST(value FROM values) THEN threshold ELSE FIRST(value FROM values) END`，不因源码把哪一臂放前面而改变。只把首项表达成 FIRST，不增加 E 或 S。

若已经取得同一数组的长度，`values.length == 0 || values.length < limit` 在 `limit` 为任意正整数时，只需要长度小于上界这一条判断；交换两个判断也一样。上界超过 Java 的长整数仍按精确业务数处理。若第二个判断读的是未证明与它相同的另一数组，就不能删掉第一个判断。
