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
diff --git a/src/total.js b/src/total.ts
rename from src/total.js
rename to src/total.ts
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

入口可静态定位，执行原生调用时保留调用者提供的对象和成员名。真实调用到达 `source.read()` 后，发现它没有随工程交付的实现或合法外部合同，按 [AI-05](../../../chapters/04-AI救赎.md#ai-05-预处理停在哪儿) 报 `Panic: not-support`，定位实际调用，不产生 PERS。不能把 await 本身当作非法，也不能在普通类型和名称校验通过后把已证明不可达的调用伪造成一次执行失败。

例如本例的定位是 `src/total.ts` 中 `total` 的 `source.read()`。
