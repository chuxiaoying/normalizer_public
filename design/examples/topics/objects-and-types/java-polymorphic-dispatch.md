# Hello World：对象多态分派

## 设计意图

语法准入先拒绝接口构造器。合法接口的静态方法或默认实现有各自的绑定。是否能分析取决于本次调用实际目标有无源码，不能只看接口的声明形状。

区分源码闭合的接收者集合和调用者传来的开放接口。已知分支按具体覆盖方法执行；局部转存开放对象不会补齐源码或授权 S。

没有被调用的其他接口方法不参与本次分派。若声明和每个实现都增加名称不同的 `preview`，入口仍只调用 `apply`，整个 PERS 不变。

一个实现可以声明两个普通接口；给 FixedFee 加上独立的 AuditedFee 不改变原 Fee 分派。合法的默认方法本体属于原生语义：调用确定落在它上面时执行其本体，不把它当抽象声明；目标未闭合时才明确拒绝。

## 输入原始代码

开放入口保留调用者传入的接口实现，不假设仓库里列出的类穷尽所有实现。封闭入口单独观察两种源码可见实现之间的分派。

```java
interface Fee { int apply(int value); }
class FixedFee implements Fee { public int apply(int value) { return value - 1; } }
class RateFee implements Fee { public int apply(int value) { return value / 2; } }
class ScaledFee implements Fee { public int apply(int value) { return value * 2; } }
class Billing {
    static int charge(Fee fee, int value) { return fee.apply(value); }

    static int chargeClosed(boolean fixed, int value) {
        Fee fee = fixed ? new FixedFee() : new ScaledFee();
        return fee.apply(value);
    }
}
```

## AI 预处理结果

**封闭入口保留原样。** 开放入口的调用目标未闭合，保留原样只用于拒绝测试，不能把源码中可见的实现当成调用者实际传入的对象。

## 归一化最终结果

### 开放入口

调用者传入 `Fee fee` 对象和 `int value`。`fee.apply(value)` 的具体实现不随规范源码交付。虽然源码列出几个实现，但不能推定 `fee` 必属于其中一个。接口签名没有声明合法 external 边界，故本例必须 Panic；不返回 PERS，也不凭空生成 S.call。

按 [AI-05](../../../chapters/04-AI救赎.md#ai-05-预处理停在哪儿) 报 `Panic: not-support`，定位 `Billing.charge` 中的 `fee.apply(value)`。

### 封闭入口

```yaml
schema: normalizer.pers/1
entry: { function: src/Billing.java#Billing.chargeClosed, returns: number }
P:
  parameters:
    fixed: { position: 0, type: bool }
    value: { position: 1, type: number }
R: CASE WHEN fixed THEN value - 1 ELSE value * 2 END
```

对于 value=4，封闭入口在 fixed=true 时选 FixedFee，返回 3；在 fixed=false 时选 ScaledFee，返回 8。RateFee 不参与这个入口的分派。正向变体分别固定两种接收者，但保留经接口的原调用。

分派依据是源码里完整的名义声明，不能把 `implements Fee<String>` 擦成 `implements Fee` 再进入封闭分派。带类型实参的父类或接口只有在实际接收者、签名和源码本体都闭合时才可分析；不得靠擦除声明猜出分派。不识别的顶层 token 也不能跳过：合法的空声明 `;` 不改变结果，任意未知单词则让同一编译单元解析失败。

已闭合的接收者选择也可以嵌套。内外重复使用同一个条件时，矛盾路径上的实现不参与执行；若两个不同的具体接收者调用后都得到 7，R 只有 7，而不是依赖接收者身份的分支。反过来，把开放接口入参赋给同类型局部变量仍无实现源码，不能生成 S 或返回 PERS。

若 FixedFee 同时实现两个各自定义同签名默认方法的接口，只有实际声明的覆盖体可以消解冲突。保留 FixedFee 自己的覆盖体时，分派仍只执行那个实现，两个默认体均不执行；若去掉覆盖体，源码有冲突，必须在准入拒绝，不能按接口书写顺序挑一个默认体。


## 显式父接口调用

```java
interface Fee { default int apply(int value) { return value + 10; } }
interface AlternativeFee { default int apply(int value) { return value + 20; } }
class FixedFee implements Fee, AlternativeFee {
    public int apply(int value) { return Fee.super.apply(value) + 1; }
}
class Billing {
    static int chargeClosed(int value) {
        Fee fee = new FixedFee();
        return fee.apply(value);
    }
}
```

**原样通过。** 外层虚调用进入 `FixedFee.apply`；里面的 `Fee.super.apply` 只调用选定的父接口实现，接收者仍是同一个对象。不会再次分派到 `FixedFee.apply`，也不会选 `AlternativeFee`。P 只有 `value: number`，R 为 `value + 11`。静态方法没有这个接收者；未直接实现的接口也不能作这个限定名，两者都是源码准入错误。


## 子接口可以继承、替换或撤回默认实现

```java
interface Parent {
    default int read() { return 1; }
}
interface Inherited extends Parent {}
interface Specific extends Parent {
    default int read() { return 2; }
}
interface Required extends Parent {
    int read();
}
class FromParent implements Inherited {}
class FromChild implements Specific {}
class Concrete implements Required {
    public int read() { return 3; }
}
class Billing {
    static int inherited() { Inherited target = new FromParent(); return target.read(); }
    static int overridden() { Specific target = new FromChild(); return target.read(); }
    static int implemented() { Required target = new Concrete(); return target.read(); }
}
```

三个入口分别返回 1、2、3。若只移除 `Concrete.read`，整个源码工程应在执行前拒绝：`Required.read` 已撤回父接口的默认实现，不能回退到 `Parent.read`。依据 [JLS §9.4.1](https://docs.oracle.com/javase/specs/jls/se25/html/jls-9.html#jls-9.4.1)。

即使变量声明为 `Parent`，只要实际对象实现 `Specific`，普通调用仍选择 `Specific.read`。默认本体内写普通调用或 `this` 调用，仍通过同一接收者虚分派；它不会变成静态地调用默认声明的接口。菱形路径若继承的是同一个声明，只算一个默认实现。
