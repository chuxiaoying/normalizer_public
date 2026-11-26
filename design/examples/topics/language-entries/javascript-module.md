# Hello World：JavaScript 模块入口

## 设计意图

检验闭合 JS 模块的标量、只读向量及有序文本 Map 合同。相同纯算术可与 Java 共用规则；稀疏、可变、重复 Map 键或改变元素位置不属于同一值。

调用方选择的入口必须是当前模块中唯一定义且显式导出的函数；仅有同名私有函数时不能把它当作入口。

## 输入原始代码

调用合同限定两个输入为业务数值，不包含字符串拼接或宿主隐式类型转换。 两个 P 都来自调用方传入的必填形参：默认值不能悄悄替调用方补 P，布尔值也不能通过 JS 加法隐式转成数字。

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

**原样通过。** 调用合同声明这是不可空、只读的数值 Vector；只读取有定义的长度，没有模拟 JS 执行或可变数组操作。因长度是自然数，正长度的补集恰为零。按[统一正则规范](../../../secondary/有序序列的长度与首项.md#已取得-typed-vector-长度的自然域)，空分支排在前，两个源码方向得到同一完整 PERS。长度下界也提供三种独立证明：`length >= 0` 恒真，`length !== -1` 恒真（负一不在自然域），`length <= 0` 则恰好等价于 `length === 0`。前两种应折叠返回值，第三种仍保留空与非空两条业务路径，并用同一个「零在前」的 CASE 表达。显式一元正号作用于精确整数字面量，例如 `+0`，不改变比较语义；不是对变量或对象做 JS 隐式数值转换。只有读入参自身的 `length` 才取得这项证据，不能把 `values.map.length` 或计算出的索引等其他属性读取当作向量长度。不把普通未知数也当作非负长度。可变的 `Array<number>` 声明也不构成只读向量证据。

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

## 有序只读 Map：可空输入只在非空时插入

三个输入都已取得，允许 null。返回值是有序的只读内容，不暴露 JavaScript Map 的可变身份。

```javascript
/**
 * @param {string|null} a
 * @param {string|null} b
 * @param {string|null} c
 * @returns {ReadonlyMap<string, string>}
 */
export function labels(a, b, c) {
  return new Map([
    ...(a === null ? [] : [['a', a]]),
    ...(b === null ? [] : [['b', b]]),
    ...(c === null ? [] : [['c', c]]),
  ]);
}
```

此合同要求一个原生 Map 构造和闭合的字面条目组装。数组返回值不是 Map；可空入参必须由自身的空值判断保护后才插入，不能无条件插入，也不能在该判断下用恰好同名的文本字面量替代入参。

**原样通过。** 按 a、b、c 决定插入哪些键。省略键不等于把 null 插入 Map。每个结果保留真实的插入顺序，不能对键应用标量交换律；例如文本键 "10" 先于 "2" 插入时，不能按普通 JavaScript 对象的整数键规则重排，空文本也不等同于 null。最后一次可能插入的条件最外层，依次为 c、b、a。以下是八个完整结果域。输出采用首匹配 CASE：例如第二臂到达时已经排除三项全空，因而只检查 b、c 为空就必然证明 a 非空。同理，第四臂到达时，前三臂在 c 为空的域已经排除了 a 或 b 为空，只剩两项均在。后四臂由前三臂和第四臂已排除的 c 为空域取得 c 非空，不重复该条件。每个条件内可交换的合取按共享结构键排序。

```yaml
schema: normalizer.pers/1
entry: { function: src/ordered-map.js#labels, returns: 'map<text,text>' }
P:
  parameters:
    a: { position: 0, type: 'text?' }
    b: { position: 1, type: 'text?' }
    c: { position: 2, type: 'text?' }
R: |-
  CASE
    WHEN a == null AND b == null AND c == null THEN MAP([])
    WHEN b == null AND c == null THEN MAP([["a", a]])
    WHEN a == null AND c == null THEN MAP([["b", b]])
    WHEN c == null THEN MAP([["a", a], ["b", b]])
    WHEN a == null AND b == null THEN MAP([["c", c]])
    WHEN b == null THEN MAP([["a", a], ["c", c]])
    WHEN a == null THEN MAP([["b", b], ["c", c]])
    ELSE MAP([["a", a], ["b", b], ["c", c]])
  END
```

本切片只接受当前入口中可认证的原生 Map 构造器、不同的字面量文本键、已取得可空文本和明确的 null 保护。不把同名实现、重复键覆盖、回调、可变引用或隐式转型解释为纯有序构造。超出展开预算时明确不产生 PERS。
