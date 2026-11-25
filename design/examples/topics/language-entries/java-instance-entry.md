# Hello World：无状态实例方法入口

## 输入原始代码

入口是 Java 实例方法，但方法没有读取或写入对象字段。实例 receiver 不因为语法上的 `this` 自动进入状态集合。

```java
class InstanceEntry {
    int add(int value, int fee) {
        return value + fee;
    }
}
```

## AI 预处理结果

**原样通过。** 这是合法的原生 Java 入口，没有外部边界或显式 `state` 标记需要补充。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/InstanceEntry.java#InstanceEntry.add
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    fee:
      position: 1
      type: number

R: fee + value
```

**预处理要点**：只要源码没有显式状态语义，AI 不为 `this` 添加隐含的 S，也不改写合法入口。

**PERS 验收要点**：入口形态和业务状态是两个层次，receiver 只有在显式状态合同下才进入 S。
