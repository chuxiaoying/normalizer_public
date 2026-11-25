# 总布尔同类项：排列与括号不改变一次判断

## 输入原始代码

三个输入都是原生布尔值，已在入口取得。判断发货须同时满足三个条件；只读这些输入，不调用服务，也不观察异常。

```java
class CanonicalChecks {
    static boolean approved(boolean address, boolean balance, boolean consent) {
        return (balance && address) && consent;
    }
}
```

## AI 预处理结果

**原样通过。** AI 不替内核打平或排序这些原生布尔条件。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalChecks.java#CanonicalChecks.approved, returns: bool }
P:
  parameters:
    address: { position: 0, type: bool }
    balance: { position: 1, type: bool }
    consent: { position: 2, type: bool }
R: address AND balance AND consent
```

三个原生 Boolean 都是总值；展开同类节点后按业务路径升序，固定左折叠。同值重复可以删除。OR 是另一种可交换的总布尔运算，换成 OR 后也按相同顺序排列，但值不同：只有 consent 为真时，AND 假、OR 真。此种子不许对尚未取得、受短路保护的观察作同样改写；见[短路种子](../control-flow/java-short-circuit.md)。
