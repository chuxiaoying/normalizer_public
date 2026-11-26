# Hello World：TypeScript 异步入口的无源码调用

## 设计意图

已解析出的 TypeScript 调用继续以真实源位置诊断：合法的 `$` 标识符不构成 external 合同，也不改变无源码调用应明确拒绝的规则。这里不产生 PERS 绑定名。

检验独立模块不把无源码对象方法当正常异步调用。await 只是等待真实完成，不声明外部合同；若到达普通入参上的无源码方法应 Panic，不捏造回执或 R。 同一个立即执行的调用去掉 await 后仍缺同一份源码或外部合同；应保留实际接收者和成员，以同步 call 诊断退出，不产出 PERS。

该准备入口只用可静态确认的封闭局部绑定报告实际调用。可变绑定或解构等待结果不能被改写成同一个闭合完成合同；拒绝时不产出 PERS。

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

**缺少执行定义。** 先将原合同交付为明确 TypeScript 声明，再保留同一输入与调用供拒绝测试。类型签名不能补造 `read()` 的方法体或外部边界。

```diff
--- a/src/total.js
+++ b/src/total.ts
@@ -1,5 +1,4 @@
-/** @param {{ read: () => Promise<number> }} source @returns {Promise<number>} */
-export async function total(source) {
+export async function total(source: { read: () => Promise<number> }): Promise<number> {
     const value = await source.read();
     return value + 1;
 }
```

## 归一化最终结果

独立模块适配器从入口语法提取立即执行的 `source.read()`，保留调用者提供的对象和成员名。内核发现它既没有源码定义，也没有合法 external 合同，必须 `Panic: not-support`，报告不能执行的调用位置，不产生 PERS。该诊断并不表示独立模块已支持一般异步执行；无法证明调用必达时，普通异步预处理缺口不能冒充这一诊断。

```text
ABORT
total:
Panic: not-support — src/total.ts#total 中的 source.read 缺少执行定义
```
