# Hello World：观察一次点击，不把事件登记算进去

## 设计意图

页面初始化只负责创建状态和登记回调。这里归一化的是宿主已经分发的一次点击：接收这次事件给出的两个值，只记录本次回调对现存状态的写入。登记不算作这次调用的外部动作，也不能因此漏掉实际写入。

## 输入原始代码

`src/Click.ts`：

```ts
declare const button: { onClick(callback: (reset: boolean, value: number) => void): void };
const counter = {count: 0};

button.onClick((reset, value) => {
  counter.count = reset ? 0 : value;
});
```

## AI 预处理结果

去掉只在安装阶段运行的登记，把被登记的闭合函数体交成具名导出；状态仍是页面已经持有的状态。这里只改观察入口，不改回调内部的选择和写入。

```diff
--- a/src/Click.ts
+++ b/src/Click.ts
@@ -1,6 +1,6 @@
-declare const button: { onClick(callback: (reset: boolean, value: number) => void): void };
+/** @N entryState */
 const counter = {count: 0};

-button.onClick((reset, value) => {
+export function onClick(reset: boolean, value: number): void {
   counter.count = reset ? 0 : value;
-});
+}
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Click.ts#onClick, returns: void }
P:
  parameters:
    reset: { position: 0, type: bool }
    value: { position: 1, type: number }
E:
  counter:
    kind: state
    source: src/Click.ts#counter
    fields: { count: number }
S:
  - write: { target: counter.count, value: CASE WHEN reset THEN 0 ELSE value END }
R: void
```

`reset` 为真写 0，否则写这次点击携带的 `value`，两条路径都确实写一次。`counter` 不是形参。若还导出一个看起来相似的 `onHover`，本次并不调用它；若交付输入仍有未被关闭的模块登记或状态标记丢失，应拒绝而不是假设宿主行为。

[第七章的对应原例](../../../chapters/07-PERS登场.md)：第七章的同步事件直接通过模块入口验收：按顺序写两次页面状态，正常返回 void。
