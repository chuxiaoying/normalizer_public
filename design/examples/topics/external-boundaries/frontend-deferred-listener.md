# Hello World：登记回调与真正发送通知

[本主题](README.md)

## 输入原始代码

一次账单处理可以登记监听而暂不派发。这里监听器的实现可见，局部回调不会逃出入口；登记回执与通知是两个均正常完成的外部操作。

```typescript
declare const audit: { registered(): void }
declare const notices: { notify(): void }

export function handle(deliver: boolean): void {
    let pending: (() => string) | undefined
    function listen(callback: () => string) {
        pending = callback
        audit.registered()
    }
    listen(() => { notices.notify(); return "callback-only" })
    if (deliver) pending!()
}
```

## AI 预处理结果

保留 listen、闭包和条件调用，只在 audit 和 notices 的声明上分别标记 @external。源码保证 listen 已给 pending 赋值，然后才检查 deliver。回调体在实际调用点进入分析。

```diff
--- a/src/handler.ts
+++ b/src/handler.ts
@@ -1,2 +1,4 @@
+/** @external */
 declare const audit: { registered(): void }
+/** @external */
 declare const notices: { notify(): void }
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry: { function: src/handler.ts#handle, returns: void }
P: { parameters: { deliver: { position: 0, type: bool } } }
S:
  - call: { external: audit.registered, args: [], returns: void, result: $e_0 }
  - call: { external: notices.notify, args: [], returns: void, result: $e_1, when: deliver }
R: void
```

**判别例**　deliver 为假时只记录 registered；为真时依次记录 registered、notify。两条路径外层都正常无返回值。
