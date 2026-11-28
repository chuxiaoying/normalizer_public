# 后置判断：循环体至少执行一次

## 设计意图

按[符号执行](../../../../design/chapters/01-符号执行.md#se-01-按源码执行并维护真实状态)的源码顺序，`do … while` 先执行循环体，再检查是否继续。判断为字面量 `false` 足以证明最多执行一次，不需要 AI 展开或改写。未知入参不自动等于无上界；如果循环条件可能永久为真且没有退出证明，则按[循环边界](../../../../design/chapters/04-AI救赎.md)报告无法直接归一。

## 输入与预处理

```java
class PostTest {
    static int run() {
        int total = 0;
        do {
            total += 2;
        } while (false);
        return total;
    }
}
```

**原样交付。** 循环体将 0 加到 2 后结束。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/PostTest.java#PostTest.run, returns: number}
P: {parameters: {}}
R: "2"
```

## 必须保留的差异

- 条件为字面量 `false`，也不能跳过第一次循环体。
- `break` 离开循环，`continue` 到达后置判断并因 `false` 离开。两条路径各自保留此前的写入，都不执行循环体的剩余语句。
- 嵌套 `switch` 的 `break` 只离开 `switch`，其中的 `continue` 仍指向外层循环。内层循环也只接自己的跳转。
- `return` 完成方法；先前挂起的循环跳转不跳过 `finally`。
