# Hello World：JavaScript 异步入口

## 输入原始代码

`source` 是调用者传入的普通对象，不是全局对象，也没有声明为合法 external 边界。JSDoc 只给出 `read()` 的返回类型，未交付方法体或外部调用合同。

```javascript
/** @param {{ read: () => Promise<number> }} source @returns {Promise<number>} */
export async function total(source) {
    const value = await source.read();
    return value + 1;
}
```

## AI 预处理结果

**缺少执行定义。** 保留输入与调用供拒绝测试。不得从 JSDoc 补造 `read()` 的方法体或边界。

## 归一化最终结果

独立模块适配器从入口语法提取立即执行的 `source.read()`，保留调用者提供的对象和成员名。内核发现它既没有源码定义，也没有合法 external 合同，必须 Panic，只给不支持诊断，不产生 PERS。该诊断并不表示独立模块已支持一般异步执行；无法证明调用必达时，普通异步预处理缺口不能冒充这一诊断。

```text
ABORT
total:
[NORM-OUT-OF-SOURCE] total -> source.read: unsupported
```
