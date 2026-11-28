# Java：冻结真正提交的请求字段

变化轴是请求字段的实际读取位置。根据[AI救赎](../../../chapters/04-AI救赎.md)与[PERS登场](../../../chapters/07-PERS登场.md)，一个外部提交记录调用当时已取得的实参；它不是以后还能随引用改变的 Java 对象。入口参数、局部对象分配与真实外部动作分别观察。

## 场景与分析副本

已核实的提交合同在 `submit` 中读取请求的 `sku`、`units` 并按该顺序提交；字段是普通整数，提交不保留本地对象，也不产生声明的异常。分析副本保留本地请求身份及所有实际读写，把这条已证明的序列化交接表达成接收两个标量的外部方法。若真实实现保留共享对象、将来才读取，不能使用这份即时合同，也不能仅凭 DTO 类名猜何时读取。

```java
import normalizer.intrinsics.N;

class Draft {
    int sku;
    int units;
}

class Dispatch {
    @N.External
    private final Broker broker;

    Dispatch(Broker broker) {
        this.broker = broker;
    }

    void submit(Draft request) {
        broker.send(request.sku, request.units);
    }

    int send(int sku, int units, int revisedUnits) {
        Draft draft = new Draft();
        draft.sku = sku;
        draft.units = units;
        Draft alias = draft;
        submit(draft);
        alias.units = revisedUnits;
        submit(draft);
        return draft.units;
    }
}

class Broker {
    native void send(int sku, int units);
}

class Beans {
    static final Dispatch ENTRY = new Dispatch(new Broker());
}
```

## 完整预期

```yaml
schema: normalizer.pers/1
entry: {function: src/Dispatch.java#Beans.ENTRY.send, returns: number}
P:
  parameters:
    sku: {position: 0, type: number}
    units: {position: 1, type: number}
    revisedUnits: {position: 2, type: number}
S:
  - call:
      external: src/Dispatch.java#Beans.ENTRY.broker.send
      args: [sku, units]
      returns: void
      result: $e_0
  - call:
      external: src/Dispatch.java#Beans.ENTRY.broker.send
      args: [sku, revisedUnits]
      returns: void
      result: $e_1
R: revisedUnits
```

第一个外部调用已经把实参记录为 `[sku, units]`。别名更新同一对象后，第二次才读到新值。对象和本地写入都未逃逸，不另外生成 E 或 S。

## 对照

- 延后第一次实际读取，把它放到别名更新之后：两次提交都记录 `[sku, revisedUnits]`，但仍然是不同的动作。
- 让 `alias` 指向另一份新分配，不改原来的请求：两次都提交 `[sku, units]`，且 R 仍是 `units`。
- 若把端口改成接收 `Draft` 引用，却没有证据表明立刻读取、延迟读取或保留引用，直接交付该端口合同应被拒绝，不应按首次调用制作快照。
- 若交付合同明确按值提交整个不可变 `record`，一条实参就记为一条记录，不拆成多个实参。两次调用即使复用同一局部变量名，也分别记录各自传入的字段值；这不把可延期读取的引用端口变成按值合同。

[第七章的对应原例](../../../chapters/07-PERS登场.md)：第七章的完整 Java 入口直接验收原例：先写剩余量，后传已写的新值；失败仍保留先前写入。
