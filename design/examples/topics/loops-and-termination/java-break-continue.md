# Hello World：循环中的 continue、break 与循环后汇合

## 设计意图

区分跳过当前迭代、退出整个循环和方法返回。退出后的汇合读取只看到真正执行过的迭代；原生跳转必须实际执行，不能当无操作或改写成不保留前缀的集合操作。

## 输入原始代码

循环跳过前两个索引，遇到大于四的索引提前终止，其余索引累加。

```java
class BreakContinue {
    static int sum(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            if (i < 2) continue;
            if (i > 4) break;
            total = total + i;
        }
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** `i` 每轮递增一次；即使 `limit` 未知，走到 `i = 5` 也确定由 `break` 退出。最多进入 6 轮，保留原生 `continue` 和 `break`，由内核沿实际路径执行。此处不把 Java 标签式跳转推断为本例的单层循环控制。

**预处理要点**：有可证明的小上界，不需要把已合规的循环交成 fold 指令；控制动作也不能被预处理粗略改成筛选。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/BreakContinue.java#BreakContinue.sum
  returns: number

P:
  parameters:
    limit: { position: 0, type: number }

R: |-
  CASE
    WHEN limit <= 2 THEN 0
    WHEN limit == 3 THEN 2
    WHEN limit == 4 THEN 5
    ELSE 9
  END
```

**PERS 验收要点**：循环化简必须保留控制动作对已提交前缀的影响，不能只看最终遍历集合，也不能把 break 后的元素算进结果。

若保留跳过 `i < 2`，并把停止条件改为 `i > 1`，第一个可尝试的候选 2 已越过停止界。任何输入都不向初始总数添加元素，R 恒为 0。
