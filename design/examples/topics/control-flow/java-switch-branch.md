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

文本标签按规范码点排序：`U+E000` 在 `U+10000` 之前，不沿用宿主 UTF-16 码元的比较顺序。合法代理对先合成一个字符；孤立的 `U+D800` 与 `U+D801` 都保留，仍是两个不同的标签，不能一起替换成 `U+FFFD`。


## 箭头分支仍是原生语句

```java
class SwitchMode {
    static int classify(int code) {
        int result = 0;
        switch (code) {
            case 1 -> result = 10;
            case 2 -> { result = 20; }
            default -> {}
        }
        return result;
    }
}
```

**原样通过。** 箭头右侧是赋值语句或块，执行选中的分支后直接继续到 `return result`，不会落入下一分支。因此它与原例交付同一整份 PERS。作为 **语句** 的 `switch` 不接受 `case 1 -> 10;` 这样的裸值；同一个 `switch` 也不能混用冒号组和箭头规则。见 [JLS §14.11.1–3](https://docs.oracle.com/javase/specs/jls/se25/html/jls-14.html#jls-14.11.1)。

如果同一分支写 `case 1, 3`，命中任一值都执行同一个分支体。没有两份执行或两处异常源；异常守卫是 `code == 1 OR code == 3`。若输出是在命中时返回 7、默认返回 0，整棵二分 CASE 的两种方向成本相同，按[统一定形](../../../chapters/08-统一定形.md#cf-04-完整二分-case)，`AND` 排在 `OR` 前，故写成 `CASE WHEN code != 1 AND code != 3 THEN 0 ELSE 7 END`。重复标签仍须跨整个 `switch` 检查，不能因标签分属不同逗号列表而放行。

编译期闭整数表达式可以作为标签。例如 `case 1 + 1` 匹配 `2`；若同一个 `switch` 又写了 `case 2`，前置校验按折叠后的值拒绝重复。这里的示例不涉及 Java 固定字宽的回卷。


## 冒号语句组可以贯穿

```java
class SwitchMode {
    static int classify(int code) {
        int result = 0;
        switch (code) {
            case 1: result = 10;
            case 2: result += 5; break;
            default: result = 1;
        }
        return result;
    }
}
```

**原样通过。** `code = 1` 先赋 10，再执行下一组的加 5，结果为 15。直接命中 `code = 2` 时结果为 5，其他输入走默认得到 1。在第一组补一个 `break` 后，只有 `code = 1` 的结果变成 10。默认组即使写在中间，也只在没有标签命中时直接选中；从前一组贯穿到默认组则照常执行。已经返回或抛出的路径不进入后面的组。


### 嵌套块里的 break 只退出自己的控制结构

在箭头组里，`if (stop) { result = 8; break; }` 会保留已经写出的 8，并跳过后续组内赋值。冒号组条件不成立时，其余路径仍能贯穿。嵌套 `switch` 只消费自己的退出，不能让已经退出外层 `switch` 的路径复活；循环内的无标签 `break` 仍由最近的循环消费。
