# Hello World：状态根中的局部别名保持字段来源

## 设计意图

检验局部别名保持状态根和嵌套字段来源。经别名读写同一成员应指回原 E；切换到另一个对象或另一成员后不可沿用旧路径。通过局部别名写入仍更新原 E；同一对象的其他字段写入可观察，但不能污染本字段读取。

同一状态记录的别名连续写两次时，后写读取前写后的字段值。E 保留旧值来源，S 保留两次写入与顺序，R 读取最终字段。 例如先加 2 再减 1，第一次写入原值加 2，第二次以这次写入的结果为被减数，R 读取同一差值。这里不要求将状态字段上的先加后减折成单一常量偏移；它不是统一正则规范中对总整数的先减后加规则。

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

**预处理要点**：预处理只交付状态身份，别名解析和字段投影交给统一 IR；局部引用不能被错误提升为新的业务来源。

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

**PERS 验收要点**：PERS 记录稳定的状态来源和字段路径，局部别名只改变源码写法，不改变业务身份。
