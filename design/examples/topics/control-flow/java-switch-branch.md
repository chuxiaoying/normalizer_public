# Hello World：原生 switch 由内核执行

## 设计意图

检验纯净的原生 Java switch 由内核执行。命中不同标签、默认以及相关的控制转移必须区分，不因预处理或定形改变求值顺序。

## 输入原始代码

入口按代码选择固定结果。这里的 `switch` 不依赖外部行为，分支均有明确返回。

```java
class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1:
                return 10;
            case 2:
                return 20;
            default:
                return 0;
        }
    }
}
```

## AI 预处理结果

**原样通过。** `switch` 是已写清楚的原生控制流，不改成一串 `if`。内核按 Java 执行选择表达式、匹配标签并收束各条返回路径。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/SwitchMode.java#SwitchMode.classify
  returns: number

P:
  parameters:
    code:
      position: 0
      type: number

R: |-
  CASE
    WHEN code == 1 THEN 10
    WHEN code == 2 THEN 20
    ELSE 0
  END
```

**预处理要点**：不因某种原生语法的内核实现尚未闭合，就把本应由内核承担的求值移交 AI。

**PERS 验收要点**：`code = 1` 返回 10，`code = 2` 返回 20，其他值返回 0。各分支的返回互不重叠，无环境读取、对外动作或异常完成。交换两个互斥的标签及其返回体，得到同一份 PERS；精确整数标签以数值排序，不以源码位置或文本拼写排序。

## 原生文本选择按内容匹配

`switch (mode)` 的选择器是原生 `String` 时，`case "ready"` 匹配内容，不拿 Java 的引用 `==` 代替。非空输入在匹配时返回 1，其余返回 0；若声明 `@N.Nullable String mode`，null 抛出 `NullPointerException`，不会选择默认分支。用 Unicode 转义写出同一文本也不能作为第二个标签。按[预处理停在哪儿](../../../chapters/04-AI救赎.md#ai-05-预处理停在哪儿)，这些原生语句交给内核执行，不改写成一串 `if`。

若两个独立的文本分支分别是 `"paused" → 2` 和 `"ready" → 1`，调换源码中的位置不改变任何输入的命中结果。只有在标签互斥且观察不变时，最终的 CASE 才按[共享结构与顺序](../../../chapters/08-统一定形.md#cf-02-共享结构与顺序)把 `mode == "paused"` 放在 `mode == "ready"` 之前。标签与结果始终一起移动；换的是标签对应的返回值，PERS 就必须不同。
