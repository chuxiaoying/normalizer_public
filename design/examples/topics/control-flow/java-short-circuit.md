# Hello World：短路布尔条件

## 设计意图

检验左侧真假决定右侧是否真实求值。右侧包含有副作用或可能失败的操作时，短路必须保留；只有证明两侧已取得且总定义时，才可按纯布尔规则改写。短路 OR 的后续发布值或效果条件同样带取得域：已为真时不读取端口，另一分支才允许引用该次结果。后续调用的实参快照与是否执行是两种独立观察。

## 输入原始代码

只有激活状态且数值为正时入口才允许继续。

```java
class Guard {
    static boolean allowed(boolean active, int value) {
        return active && value > 0;
    }
}
```

## AI 预处理结果

**原样通过。** Java 的短路条件是当前 parser 可处理的原生控制表达。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Guard.java#Guard.allowed
  returns: bool

P:
  parameters:
    active:
      position: 0
      type: bool
    value:
      position: 1
      type: number

R: active AND value > 0
```

**预处理要点**：能由工程解析的短路表达式不需要 AI 改写，工程负责保持条件的业务组合关系。

**PERS 验收要点**：PERS 统一布尔语义，同时保留短路条件的逻辑依赖，不按语言分裂结果合同。

## 有外部观察的短路对照

如果 active 为 false，不应调用 probe；仅验收返回 false 无法区分“未调用”和“调用后仍返回 false”。本例端口约定正常返回数值。

```java
import normalizer.annotations.External;

class GuardWithProbe {
    @External
    private final Probe probe;

    GuardWithProbe(Probe probe) { this.probe = probe; }

    boolean allowed(boolean active, int value) {
        return active && probe.read(value) > 0;
    }
}

interface Probe { int read(int value); }
```

```yaml
schema: normalizer.pers/1
entry: { function: src/GuardWithProbe.java#GuardWithProbe.allowed, returns: bool }
P:
  parameters:
    active: { position: 0, type: bool }
    value: { position: 1, type: number }
S:
  - call: { external: probe.read, args: [value], returns: number, result: $e_0, when: active }
R: CASE WHEN active THEN $e_0 > 0 ELSE false END
```

派生对照先主动调用再判断，故 active 为 false 时两者都返回 false，但只有对照会调用端口。

另一条等价对照写成 `!(!active || probe.read(value) <= 0)`。未激活时不读取端口；激活时只调用一次，返回值和调用取得域都与原例一样。
