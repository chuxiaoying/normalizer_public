# Hello World：多个参数与 CASE-WHEN

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

**原样通过。** 局部变量、两个条件判断和三个返回点都由 Java 语法直接表达。上面的 Java 即为 Normalizer 的输入。

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 将整个方法替换成 `Meaning.choose(...)`，再在 `project.yaml` 中逐步定义 `adjusted`、`gap`、两次判断和三个返回；new 保留原生 Java，直接交给 Normalizer。 | 原生语法已经表达清楚的计算与控制流应由工程理解。AI 只补工程缺少的局部语义，不为普通业务方法另写一份完整程序。 |

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

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 先计算 `adjusted - limit` 再与零比较；new 直接比较 `limit` 与 `value + 1`，相等时仍独立返回 `fallback`。 | 把分支归约为输入之间的直接关系，同时保留产生不同结果的条件；消去中间计算不能抹掉业务分界。 |
| old 的末支保留加一再减一；new 的末支直接返回 `value`。 | 归一化要继续消除路径内相互抵消的运算，让每个分支给出最简结果。 |
