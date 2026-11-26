# Hello World：原生异常进入对应的 catch

## 设计意图

检验正常路径、能命中处理器的异常路径与真正逃逸的异常路径各自保留。不能在预处理阶段跳过 throw 或 catch。

## 输入原始代码

入口在负数输入时显式抛出异常，并在入口内捕获后返回 fallback。正常路径和异常路径都是业务行为。

```java
class NegativeValue extends RuntimeException {}

class Guarded {
    static int read(int value) {
        try {
            if (value < 0) throw new NegativeValue();
            return value;
        } catch (NegativeValue ignored) {
            return 0;
        }
    }
}
```

## AI 预处理结果

**原样通过。** 项目内异常和处理器都有完整定义。只有内核实际执行到负数分支时才抛出，再按原有顺序选中 catch；不能用条件返回代替分析源码中的原生异常行为。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/Guarded.java#Guarded.read, returns: number }
P: { parameters: { value: { position: 0, type: number } } }
R: |-
  CASE
    WHEN value > 0 THEN value
    ELSE 0
  END
```

负数路径抛出的异常已在入口内部捕获，不进入顶层 `throws`，R 保留了这条路径返回的 0；输入正数时返回原值。输入恰好为 0 时虽然源程序走正常路径，但两个结果都是 0，因此可以并入返回 0 的分支。若不捕获该异常，负数域须记到 `throws`，不能同时宣称这条路径在 R 中返回 0。
