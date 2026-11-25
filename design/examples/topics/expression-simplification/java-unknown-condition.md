# Hello World：未知布尔条件保留为路径条件

## 输入原始代码

入口的 `enabled` 来自调用者。Normalizer 不能预先选择任一分支，两个分支的业务表达都必须保留。

```java
class Unknown {
    static int choose(boolean enabled, int value) {
        return enabled ? value : value + 1;
    }
}
```

## AI 预处理结果

**原样通过。** 三元表达式和布尔参数属于当前 parser 可处理的原生 Java。AI 不把未知条件改成常量，也不替工程提前选择路径。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Unknown.java#Unknown.choose
  returns: number

P:
  parameters:
    enabled:
      position: 0
      type: bool
    value:
      position: 1
      type: number

R: CASE WHEN enabled THEN value ELSE value + 1 END
```

**预处理要点**：未知输入不是预处理阶段的待定常量，AI 应把完整数据流交给符号化执行。

**PERS 验收要点**：归一化结果表达业务函数的全部可达路径，不能用某个示例输入代替条件本身。
