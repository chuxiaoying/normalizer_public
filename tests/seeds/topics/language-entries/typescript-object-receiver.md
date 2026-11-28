# Hello World：从导出对象进入方法，保留 this

## 设计意图

入口 `src/main.ts#controller.read` 定位的是整个调用，不是一个可以脱离接收者执行的函数名。对象是工程内显式装配的；核对导出身份，再使用该对象的字段值。

## 输入原始代码

`src/main.ts`：

```ts
export const controller = {
  base: 7,
  read(value: number): number { return this.base + value; }
} as const;
```

## AI 预处理结果

**原样通过。** 这是严格 TypeScript。接收者是没有初始化副作用的闭合字面量。同一工程没有后续写入；`base` 是已证明固定的装配值，不是本次调用的参数，也不是要在入口读取的未知状态。不得把 `read` 从对象上拆下来或用同名对象替换。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/main.ts#controller.read, returns: number }
P:
  parameters:
    value: { position: 0, type: number }
R: value + 7
```

若另导出 `other`，它也有同名 `read`、但配置值是 13，则 `src/main.ts#other.read` 返回 `value + 13`。直接定位 `#read` 不能取得对象，必须拒绝且不生成 PERS。未导出的对象也不能用工程内的同名定位偷渡。

若方法把 `this.base + value` 传给已绑定的工程内 `plusOne`，它仍从 `controller` 取得 7，再执行实际的加一，结果是 `value + 8`。辅助文件位于子目录时，在经过工程根检查的相对路径内回到上一级，不会把接收者换成辅助文件里的对象。
