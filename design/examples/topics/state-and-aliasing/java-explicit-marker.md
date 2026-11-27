# Hello World：只有显式标记的实例字段进入状态

## 设计意图

区别显式状态字段、普通局部和同名参数。只有真实标记和绑定才产生 E 与对应 S；改变字段写入次序或非交换运算的操作数可能改变结果，不用名字推断状态。

状态名称和 E 中的所属类型由同一字段决定。写出完整的标记名或使用空括号不改变 E；引入同名局部变量也不会改变 `this.value` 的接收者。

叶字段必须沿真实的名义类型解析。两种不同记录都有 units 时，写 zebra.units 不改变 amber.units；标量状态为布尔时，取反写入仍保持原字段身份和 bool 值域。

## 输入原始代码

`Counter.value` 是入口已有的可变状态。方法只读取它并加一，实例方法本身和 `this` 不自动声明状态。

```java
import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int read() {
        return this.value + 1;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
```

## AI 预处理结果

**原样通过。** 状态身份已由 `@N.EntryState` 标记交付，入口从已装配的 `Beans.ENTRY` 取得对应实例。AI 不把实例方法改写成静态函数，也不额外生成 receiver 参数。

**预处理要点**：状态边界由显式标记建立。没有标记时，字段、`this` 和类型名都不能让工程猜测系统状态；已经有标记时，预处理不再改写宿主语言结构。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Counter.java#Beans.ENTRY.read
  returns: number

P:
  parameters: {}

E:
  value:
    kind: state
    source: src/Counter.java#Beans.ENTRY.value
    type: number

R: value + 1
```

**PERS 验收要点**：E 记录入口已有状态的来源，P 只记录调用输入。状态与参数的职责由标记和来源合同决定，不靠 `this` 的语法形式推断。把只读入口改成先执行 `this.value = this.value + 1` 再返回 `this.value` 时，E 仍记录入口的旧状态，S 恰有一次写入，R 读到刚写入的值；写入的依据是原字段和实际执行，而非接收者拼写。

## 两条已取得的状态字段按公开路径排序

```java
import normalizer.intrinsics.N;
class Quota { int units; }
class Counter {
    @N.EntryState Quota zebra;
    @N.EntryState Quota amber;

    int readPair() {
        return zebra.units + amber.units;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
    static {
        ENTRY.zebra = new Quota();
        ENTRY.amber = new Quota();
    }
}
```

**原样通过。** 两个状态均为入口已取得的对象，不读真正的端口，不执行写入。两条数值可以在计算结构中交换，记录仍各自引用原状态字段。调换源码读取或字段声明的先后，不改变唯一 R。

```yaml
schema: normalizer.pers/1
entry: { function: src/Counter.java#Beans.ENTRY.readPair, returns: number }
P:
  parameters: {}
E:
  zebra: { kind: state, source: src/Counter.java#Beans.ENTRY.zebra, fields: { units: number } }
  amber: { kind: state, source: src/Counter.java#Beans.ENTRY.amber, fields: { units: number } }
R: amber.units + zebra.units
```

装配若改为 `new Counter(7)`，且构造器用参数给已声明的 `this.value` 赋值，仍选择同一已装配入口。7 属于启动时的初值，不是每次调用前的值。P 仍为空，E 仍是当次 `Beans.ENTRY.value`，R 为 `value + 1`，本次调用不产生构造器写入。

重复导入同一个 `normalizer.intrinsics.N` 不会改变标记的归属；这和两个不同类型争用名字 `N` 不同。
