# Hello World：等式与不等式联合约束

## 设计意图

检验等式与不等式共同约束的有效域。相同解集可有唯一写法；相互矛盾、端点严格性变化及不同业务变量不能被同一个形状掩盖。

## 输入原始代码

入口同时要求等式成立，并要求目标值不超过上限。三个输入保持未知，不能在预处理阶段代入具体数字求解。

```java
class System {
    static boolean consistent(int value, int target, int limit) {
        return value + 1 == target && target <= limit;
    }
}
```

## AI 预处理结果

**原样通过。** 加法、等式、不等式和布尔合取属于当前 parser 可处理的原生 Java。AI 不替工程求解输入，也不删除任何约束。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/System.java#System.consistent
  returns: bool

P:
  parameters:
    value:
      position: 0
      type: number
    target:
      position: 1
      type: number
    limit:
      position: 2
      type: number

R: limit >= target AND target == value + 1
```

**预处理要点**：约束系统交给内核做规范化和后续演算，AI 不把未知输入提前求成常量。

**PERS 验收要点**：等式方向、不等式方向和布尔连接词都进入唯一形式，联合约束的断言不依赖源码书写顺序。

等比方程也是对同一输入域的重复声明。例中 `value + target == 10` 与 `2 * (value + target) == 20` 对每对精确整数同时成立或不成立；合取仍只表示这一个约束。保留三个入口参数，也不为无穷多个解挑一个特定值。
