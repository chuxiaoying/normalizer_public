# Hello World：多个参数与 CASE-WHEN

## 设计意图

检验多参数分支的有效域、默认值和唯一 CASE 表示。交换纯条件的等价写法不应改变结果；调整边界或前置分支会改变哪些输入选中哪一臂。

## 输入原始代码

这个案例保持纯计算，没有状态写入、环境读取、external 调用或异常路径。它增加多个入口参数、局部变量和多个源代码返回分支，重点展示多个返回路径如何收束为一个业务级 `R`。

```java
class Decision {
    static int choose(int value, int limit, int fallback) {
        int adjusted = value + 1;
        int gap = adjusted - limit;

        if (gap > 0) {
            return adjusted;
        }
        if (gap == 0) {
            return fallback;
        }
        return adjusted - 1;
    }
}
```

## AI 预处理结果

**原样通过。** 局部变量、两个条件判断和三个返回点都由 Java 语法直接表达。

**预处理要点**：原生语法已经表达清楚的计算与控制流应由工程理解。AI 只补工程缺少的局部语义，不为普通业务方法另写一份完整程序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Decision.java#Decision.choose
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number
    limit:
      position: 1
      type: number
    fallback:
      position: 2
      type: number

R: |-
  CASE
    WHEN limit < value + 1 THEN value + 1
    WHEN limit == value + 1 THEN fallback
    ELSE value
  END
```

三个 `return` 按源码分支顺序进入 `R`；局部变量 `adjusted` 和 `gap` 的取值已代入对应的判断和结果。

**PERS 验收要点**

- 把分支归约为输入之间的直接关系，同时保留产生不同结果的条件；消去中间计算不能抹掉业务分界。
- 归一化要继续消除路径内相互抵消的运算，让每个分支给出最简结果。

### 已保护的整数读取也能证明分支互补

先处理 `box == null` 返回 3，随后按已取得的 `box.value > 1` 返回 5 或 9。顺序早退、显式嵌套，以及以 `box.value < 2` 反向判断，都应得到相同的 `CASE WHEN box == null THEN 3 WHEN 2 > box.value THEN 9 ELSE 5 END`。因为只有空值分支失败后才读取字段，且这时字段已是同一个精确整数；相邻端点之间没有值。不能把这个结论搬到未取得字段的路径，更不能把 `box.value > 1` 和另一个字段 `box.other < 2` 当作互补。若原源码未做空值检查，则照常保留 `NullPointerException`。

三路也要证明覆盖，而不能只认出一对互补条件。若非空后依次区分 `box.value > 1`、`box.value < 1`，余下恰为 `1`，用最终的 ELSE 表达。若把第二阈值改成 `0`，余下变成 `[0, 1]`，两值都必须得到真实兜底。没有前置空检查时，整数分区仍覆盖正常域，但不可把 `box == null` 的异常算进正常返回。

`box != null && box.value > 1` 的短路写法，随后单独处理 null，也给出同一个结果。若右操作数是一个先写 `box.other` 再比较的辅助方法，定形只能统一 R；S 仍恰好在 `box != null` 时写一次，包括比较为假的非空路径。
