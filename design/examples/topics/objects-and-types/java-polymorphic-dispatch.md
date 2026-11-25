# Hello World：对象多态分派

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
