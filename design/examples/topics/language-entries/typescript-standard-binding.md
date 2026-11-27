# TypeScript：用官方标准库处理显示标签

变化轴是调用绑定，而不是 `N` 或 `trim` 这个名字。按[第四章](../../../chapters/第四章%20·%20AI%20救赎.md)与[标准操作合同](../../../secondary/标准操作与声明标记.md)，原分析模块里的 namespace import 必须真的解析到官方库，调用也必须匹配官方签名。这里只交付两项已有闭合定义的文本操作，不据此假定其他库成员、动态访问、或普通对象方法可执行。

## 已准备的输入

展示标签先去掉两端 UTF-16 码元值不大于 32 的字符，再在结果为 null、空串或只含 Java 17 空白码元时采用调用者给的默认标签。两个输入均由入口声明提供，不读取宿主，也不写状态。

```ts
import * as N from "@normalizer/stdlib";

export function normalized(raw: string | null, fallback: string): string {
  return N.defaultIfBlank(N.trim(raw), fallback);
}
```

## 完整预期

```yaml
schema: normalizer.pers/1
entry:
  function: src/labels.ts#normalized
  returns: text
P:
  parameters:
    raw: {position: 0, type: text?}
    fallback: {position: 1, type: text}
R: |-
  CASE
    WHEN blank(TRIM(raw)) THEN fallback
    ELSE TRIM(raw)
  END
```

原始输入保持可空，裁剪不把 null 改为空串；在 `blank` 分支之外返回裁剪后的文本。不产生 E 或 S。

## 对照

- 把 namespace 本地名改为 `Official`，输入合同和完整 PERS 不变。
- 两个标准裁剪函数嵌套仍只需一个裁剪；裁剪字符的边界固定，不能改用 JS 宿主的 `String.prototype.trim`。
- 将原调用的可空值替换为 `null` 并用固定默认标签，结果直接为该默认标签。含不换行空格的文字不是空白标签。
- 入口形参 `N: string` 遮蔽导入时，`N.trim()` 实际是宿主对象方法，不是官方操作；不产出 PERS。
- 缺失导入、错误签名和非 namespace 绑定均拒绝，不产出 PERS。
- 本地相对模块的文本辅助函数也可自己绑定官方 namespace；把 `trim` 放到这个闭合的辅助函数时完整 PERS 不变。项目依赖缺失或辅助函数的参数遮蔽官方 namespace 时，另一文件的有效导入不能补上这项证据。
