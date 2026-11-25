# Hello World：嵌套记录字段投影

## 输入原始代码

入口接收一个包含嵌套记录的对象，只读取最内层的业务字段。

```java
class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int read(Outer outer) {
        return outer.inner.value;
    }
}
```

## AI 预处理结果

**原样通过。** 这是合法的原生 Java。AI 不把 `inner` 或 `value` 提升为新的入口参数，也不把嵌套对象改写成扁平 map。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留 `Outer → Inner → value` 的原生字段路径。 | 对象类型和字段来源交给统一 IR 建立，预处理不为嵌套字段制造额外输入或隐藏对象结构。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Inner:
    fields:
      value: number
  Outer:
    fields:
      inner: Inner

entry:
  function: src/Nested.java#Nested.read
  returns: number

P:
  parameters:
    outer:
      position: 0
      type: Outer

R: outer.inner.value
```

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只保留一条源码字段链；new 在 `types` 中完整声明 `Inner`、`Outer` 和 `Outer.inner` 的类型，并在 R 中保留 `outer.inner.value`。 | 嵌套对象的类型路径是业务来源的一部分。PERS 需要完整声明中间记录，不能只凭最终字段名猜测来源。 |
