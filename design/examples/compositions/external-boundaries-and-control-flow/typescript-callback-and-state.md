# Hello World：登记回调不写状态，实际调用才写

## 设计意图

登记只保存函数并发送一次登记审计。确实派发时，回调先把装配好的账本写为 1，再调用通知，最后写为 2。状态写入和外部调用都在统一的有序 S 中；不调用时不产生这些动作，也不能因为局部函数返回文本，就让入口返回文本。

## 输入原始代码

`src/Dispatch.ts`：

```ts
declare const audit: {registered(): void};

declare const notices: {sent(): void};

const ledger = {stage: 0};

export function dispatch(deliver: boolean): void {
  let pending: (() => string) | undefined;
  function listen(callback: () => string) {
    pending = callback;
    audit.registered();
  }
  listen(() => {
    ledger.stage = 1;
    notices.sent();
    ledger.stage = 2;
    return "callback-only";
  });
  if (deliver) pending!();
}
```

## AI 预处理结果

保留注册函数、回调、条件和回调里的源码顺序；标记既有状态和两个外部接收者。没有把条件移到登记前，也没有用一笔最终值抹掉前一笔写入。

```diff
--- a/src/Dispatch.ts
+++ b/src/Dispatch.ts
@@ -1,20 +1,23 @@
+/** @N external */
 declare const audit: {registered(): void};

+/** @N external */
 declare const notices: {sent(): void};

+/** @N entryState */
 const ledger = {stage: 0};

 export function dispatch(deliver: boolean): void {
   let pending: (() => string) | undefined;
   function listen(callback: () => string) {
     pending = callback;
     audit.registered();
   }
   listen(() => {
     ledger.stage = 1;
     notices.sent();
     ledger.stage = 2;
     return "callback-only";
   });
   if (deliver) pending!();
 }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Dispatch.ts#dispatch, returns: void }
P:
  parameters:
    deliver: { position: 0, type: bool }
E:
  ledger:
    kind: state
    source: src/Dispatch.ts#ledger
    fields: { stage: number }
S:
  - call: { external: src/Dispatch.ts#audit.registered, args: [], returns: void, result: $e_0 }
  - write: { target: ledger.stage, value: "1", when: deliver }
  - call: { external: src/Dispatch.ts#notices.sent, args: [], returns: void, result: $e_1, when: deliver }
  - write: { target: ledger.stage, value: "2", when: deliver }
R: void
```

只登记、不派发时，只发布审计，既没有账本状态记录也没有通知回执；如果登记函数主动调用回调，则三项动作确实在登记过程中发生。将通知挪到最后会改变 S 的顺序，不能按字段名或调用名排序。
