# Hello World：登记回调与真正发送通知

## 设计意图

对比登记回调、条件派发与重复派发。单纯储存回调不发通知；真分支才触发有序 S，两次命中仍是两次回执，未经声明的实际调用不能当成外部端口。同步零参 void 派发通过声明的所有者和方法绑定；同时改方法声明和调用，只更改通知端点，不改登记、派发域或回执次序。

本例的基线是不逃逸的零参登记回调和单一条件派发。派生场景若增加闭合可见的 else 动作，仍按原分支记录，不把合法的双分支限制成单分支。派发条件取业务布尔值；来自原生文本真值转换的输入必须证明并显式交付同义布尔条件，不能猜它的类型或含义。

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

保留 listen、闭包和条件调用，只在 audit 和 notices 的声明上分别标记 `/** @N external */`。源码保证 listen 已给 pending 赋值，然后才检查 deliver。回调体在实际调用点进入分析。

```diff
--- a/src/handler.ts
+++ b/src/handler.ts
@@ -1,12 +1,14 @@
+/** @N external */
 declare const audit: { registered(): void }
+/** @N external */
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

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry: { function: src/handler.ts#handle, returns: void }
P: { parameters: { deliver: { position: 0, type: bool } } }
S:
  - call: { external: src/handler.ts#audit.registered, args: [], returns: void, result: $e_0 }
  - call: { external: src/handler.ts#notices.notify, args: [], returns: void, result: $e_1, when: deliver }
R: void
```

**判别例**　deliver 为假时只记录 registered；为真时依次记录 registered、notify。两条路径外层都正常无返回值。

### 派发条件的布尔变化

登记本身不受派发条件影响。把派发条件改为 `!deliver` 只取反通知发生的域；双重取反恢复原域；`!true` 只省略实际通知，不省略登记。两处依次按 `deliver` 与 `!deliver` 派发时，每个具体输入只调用一次，但两个源码调用点各有带条件的 S 和回执。统一 IR 的派发条件仍然只有布尔。TypeScript 适配层自己消除数值常量的真假转换：`!1` 和 `!(-1)` 直接成为布尔常量假，只登记不发送；`!0`、`!0x0`、`!0.00` 直接成为布尔常量真，登记后无条件发送。适配层以字面量的精确值是否为零判断，不把 JavaScript 特有的真假转换送进 IR，也不引入外部调用。

### 回调按定义处解析名字

调用时点决定何时产生 S，但不改变回调里每个名字所指的对象。若登记函数把自己的回调形参也命名为 notices，定义在入口里的箭头函数仍读取入口可见的外部 notices；登记函数自己直接访问 notices 则读取它的形参。入口自己的局部绑定也可能遮蔽同名外部对象。只有调用接收者确实绑定到带合同的外部声明才产生该端口的 S；同名并不等于同一声明。对照检验延后派发、登记时立即调用回调和登记函数自己调用未被遮蔽的 audit。

登记也靠绑定身份，而不是变量拼写：左值必须是入口中实际持有回调的槽，右值必须是登记函数的回调形参。若登记函数的形参也叫 pending，函数里的 pending = pending 只改写该形参，不会登记入口回调。未登记时有两种边界：能证明槽仍是 undefined，且源码在 deliver 为真时强行调用它，则只有这条路径抛出 TypeError，保留此前确实发生的登记审计，不产生通知或通知回执；若闭合证据不足以确定回调绑定和状态，才报告分析失败，不能虚构任何回执。这不限制 TypeScript 本身允许同名形参或对其赋值。

严格比较交换两侧不改变等值关系。登记中已经把回调保存到 `pending` 后，不论写 `callback === pending` 还是 `pending === callback`，都只观察本次确证的同一个回调。派发时，对 `deliver ? "send" : ""` 作严格文本比较也遵循同一证据：和 `"send"` 比较只在 deliver 时派发；和 `""` 比较只在非 deliver 时派发。若两臂都给出 `"send"`，严格等于 `"send"` 恒真，通知无条件发生。以上始终分别保留登记及通知的真实顺序，不把真假转换交给 IR。
