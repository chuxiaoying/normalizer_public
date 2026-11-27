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

```text
ABORT
charge:
[NORM-OUT-OF-SOURCE] Billing.charge -> Fee.apply: unsupported
```

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
