# Hello World：JavaScript 模块入口

## 输入原始代码

调用合同限定两个输入为业务数值，不包含字符串拼接或宿主隐式类型转换。

```javascript
/** @param {number} value @param {number} base @returns {number} */
export function total(value, base) { return value + base; }
```

## AI 预处理结果

**原样通过。** 保留有源码依据的计算、类型和观察顺序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/total.js#total, returns: number }
P:
  parameters:
    value: { position: 0, type: number }
    base: { position: 1, type: number }
R: base + value
```

## 已取得的只读 Vector 长度采用自然域代表

```javascript
/**
 * @param {ReadonlyArray<number>} values
 * @returns {number}
 */
export function choose(values) {
  return values.length > 0 ? 7 : 11;
}
```

**原样通过。** 调用合同声明这是不可空、只读的数值 Vector；只读取有定义的长度，没有模拟 JS 执行或可变数组操作。因长度是自然数，正长度的补集恰为零。按[统一正则规范](../../../unified-canonical-form.md#已取得-typed-vector-长度的自然域目标合同)，空分支排在前，两个源码方向得到同一完整 PERS。不把普通未知数也当作非负长度。

```yaml
schema: normalizer.pers/1
entry: { function: src/vector-case.js#choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: |-
  CASE
    WHEN LENGTH(values) == 0 THEN 11
    ELSE 7
  END
```
