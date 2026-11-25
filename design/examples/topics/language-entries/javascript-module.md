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
