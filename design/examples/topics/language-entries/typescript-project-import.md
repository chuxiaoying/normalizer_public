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
