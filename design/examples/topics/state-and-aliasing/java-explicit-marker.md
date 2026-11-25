# Hello World：只有显式标记的实例字段进入状态

## 输入原始代码

`Counter.value` 是入口已有的可变状态。方法只读取它并加一，实例方法本身和 `this` 不自动声明状态。

```java
import normalizer.annotations.EntryState;

class Counter {
    @EntryState
    int value;

    int read() {
        return this.value + 1;
    }
}
```

## AI 预处理结果

**原样通过。** 状态身份已经由 Normalizer 自身的 `@EntryState` 标记交付。AI 不把实例方法改写成静态函数，也不额外生成 receiver 参数。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留带 `@EntryState` 的合法 Java。 | 状态边界由显式标记建立。没有标记时，字段、`this` 和类型名都不能让工程猜测系统状态；已经有标记时，预处理不再改写宿主语言结构。 |

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Counter.java#Counter.read
  returns: number

P:
  parameters: {}

E:
  value:
    kind: state
    source: src/Counter.java#Counter.value
    type: number

R: value + 1
```

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只能把实例字段看作未声明来源；new 在 E 中登记 `Counter.value`，R 直接读取该状态并计算 `value + 1`。 | E 记录入口已有状态的来源，P 只记录调用输入。状态与参数的职责由标记和来源合同决定，不靠 `this` 的语法形式推断。 |

## 两条已取得的状态字段按公开路径排序

```java
import normalizer.annotations.EntryState;
class Quota { int units; }
class Counter {
    @EntryState Quota zebra;
    @EntryState Quota amber;

    int readPair() {
        return zebra.units + amber.units;
    }
}
```

**原样通过。** 两个状态均为入口已取得的对象，不读真正的端口，不执行写入。两条数值可以在计算结构中交换，记录仍各自引用原状态字段。调换源码读取或字段声明的先后，不改变唯一 R。

```yaml
schema: normalizer.pers/1
types:
  Quota: { fields: { units: number } }
entry: { function: src/Counter.java#Counter.readPair, returns: number }
P:
  parameters: {}
E:
  zebra: { kind: state, source: src/Counter.java#Counter.zebra, type: Quota }
  amber: { kind: state, source: src/Counter.java#Counter.amber, type: Quota }
R: amber.units + zebra.units
```
