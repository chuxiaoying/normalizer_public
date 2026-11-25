# Hello World：状态根中的局部别名保持字段来源

## 输入原始代码

`Alias.box` 是显式声明的入口状态。方法先把这个状态根绑定到局部 `alias`，再通过别名读取嵌套对象字段。

```java
import normalizer.annotations.EntryState;

class Box {
    int value;
}

class Alias {
    @EntryState
    Box box;

    int read() {
        Box alias = this.box;
        return alias.value;
    }
}
```

## AI 预处理结果

**原样通过。** `@EntryState` 已建立状态根，局部 `alias` 和字段读取都是原生 Java。AI 不把别名改写成新的入口参数，也不把嵌套字段复制成独立状态。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留状态标记、局部别名和原生字段访问。 | 预处理只交付状态身份，别名解析和字段投影交给统一 IR；局部引用不能被错误提升为新的业务来源。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Box:
    fields:
      value: number

entry:
  function: src/Alias.java#Alias.read
  returns: number

P:
  parameters: {}

E:
  box:
    kind: state
    source: src/Alias.java#Alias.box
    type: Box

R: box.value
```

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 把 `alias.value` 留在局部引用上；new 将它解析为状态根 `E.box` 的字段投影 `box.value`。 | PERS 记录稳定的状态来源和字段路径，局部别名只改变源码写法，不改变业务身份。 |
