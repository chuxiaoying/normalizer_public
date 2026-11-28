# Hello World：在闭合的 TypeScript 工程里找到实际导入

## 设计意图

入口调用另一个工程文件中可静态定位的导出函数。先核对导入绑定、所有依赖的源码和类型，再执行实际被调用的函数。不能靠同名函数、声明桩或未解析的模块猜结果。

## 输入原始代码

`src/main.ts`：

```ts
import { plusOne } from "./helper.js";
export function run(value: number): number { return plusOne(value); }
```

`src/helper.ts`：

```ts
export function plusOne(value: number): number { return value + 1; }
```

## AI 预处理结果

**原样通过。** 这是严格 TypeScript。ESM 的 `./helper.js` 是编译后文件的引用；当前分析工程内真实的实现位于 `src/helper.ts`，不会改用同名宿主模块。函数没有运行时初始化、隐藏状态或外部调用。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/main.ts#run, returns: number }
P:
  parameters:
    value: { position: 0, type: number }
R: value + 1
```

用 `import { plusOne as step }` 并调用 `step(value)`，结果必须相同。入口若导入两个模块里同名的函数，则各自绑定自己的实现；不能因为两个声明都叫 `plusOne` 而合并调用。若缺失依赖、实现的类型错误、文件越过工程根或导入触发未证明的初始化，应在分析阶段拒绝，不输出 PERS。

## 项目内的默认导出

将入口改成 `import { default as plusOne } from "./helper.js"`，将帮助模块里的实现改成 `export default function plusOne(value: number): number { return value + 1; }`，得到与上面一样的完整 PERS。默认导出确实存在，局部名 `plusOne` 不是据以匹配远端名字。如果帮助模块只保留 `export function plusOne`，则没有默认导出，应拒绝且不生成 PERS。

**分析预算与输入合同分开。** 同样是已定位的纯整数帮助函数，若 `double(x)` 返回 `x + x`，嵌套两次得到四次参数使用。增加嵌套时，若当前执行器不能在预算内完成展开，应报分析未完成且不输出 PERS；输入仍然具有同一语义，不能改成「需要预处理」、截断，也不能发布部分表达式。具体上限不是设计约定。

## 类型别名按实际声明解析

入口与帮助模块都可从工程内另一个文件 `import type { Domain }`。当严格 TypeScript 检查确认 `Domain` 正是 `number`、`boolean` 或 `string` 时，P 和返回值分别仍是原有 `number`、`bool`、`text`；换一个别名不改变业务域，引用本身不产生外部观察。不能只按别名拼写猜值域，也不能扩大一个受限或混合的类型来让它通过。

具名记录同样按实际声明解析。若入口用 `import type { Quote as BillingQuote }` 绑定到另一文件的 `Quote`，则 P 引用 `Quote`，`types.Quote.fields` 来自那份声明。返回 `value.amount` 仍只是读本次参数的字段；局部导入别名不会凭空制造第二种业务类型或一个外部来源。

这与拿 `.d.ts` 给运行时函数充当实现不同。声明文件只说明签名，不能执行实际帮助函数。导入工程内的辅助模块也不能用 `@ts-nocheck` 或 `@ts-ignore` 绕过[强类型准入](../../../chapters/04-AI救赎.md#ai-04-强类型准入)；即使选定入口看起来有完整类型，也拒绝生成 PERS。

若 `import type { Payload }` 解析到 `Payload = unknown`，仍按[符号执行的宽类型规则](../../../chapters/01-符号执行.md#se-03-宽类型值按源码语义执行)执行。原样返回或抛出时保留接到的值；若 `typeof value !== "string"` 则抛出原来的 `TypeError`，只在文本路径返回同一个入参。把字符串标签换成无插值模板字面量，类型条件保持不变。单独写 `value as string` 不执行类型检查，不能当成收窄证据。

入口本身只定位一个导出绑定，不接受调用参数。应写作 `src/main.ts#run`，不能传绝对路径、带空段或点段的相对路径，也不能写成 `src/main.ts#run(3)`。这些错误在读文件前拒绝，源码即使本来合法也不能生成 PERS。
