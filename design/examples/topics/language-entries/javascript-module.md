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

## 不可变构造把完整 CASE 放到外层

```javascript
/**
 * @param {boolean} enabled
 * @param {number} left
 * @param {number} right
 * @param {number} fixed
 * @returns {ReadonlyArray<number>}
 */
export function pair(enabled, left, right, fixed) {
  return [enabled ? left : right, fixed];
}
```

**原样通过。** 出口是不可变数值内容，不观察对象身份。条件以及两个元素都来自已取得的入口参数，构造本身不抛异常、不执行外部动作。外提须保留第二个元素和元素位置，不能因某个分支未命中就多取得外部值或重排元素。

```yaml
schema: normalizer.pers/1
entry: { function: src/vector-construction.js#pair, returns: 'number[]' }
P:
  parameters:
    enabled: { position: 0, type: bool }
    left: { position: 1, type: number }
    right: { position: 2, type: number }
    fixed: { position: 3, type: number }
R: CASE WHEN enabled THEN [left, fixed] ELSE [right, fixed] END
```
