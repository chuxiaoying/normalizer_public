# 完整二分 CASE：正计数保留原值，其余归零

## 设计意图

检验完整二分 CASE 的互补条件只可同时交换两臂。正、零、负用同一标准，阈值从零改一是反例；部分域若缺默认不能臆造 ELSE。

## 输入原始代码

`count` 是已取得的入口整数。零归到默认值，正数保留原值，负数也回到默认值。两臂都是纯值，没有读取、调用或异常。

```java
class CanonicalCount {
    static int clamp(int count) {
        if (count <= 0) return 0;
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** AI 不替内核翻转条件，也不预先计算未知的 `count`。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry:
  function: src/CanonicalCount.java#CanonicalCount.clamp
  returns: number
P:
  parameters:
    count:
      position: 0
      type: number
R: |-
  CASE
    WHEN count > 0 THEN count
    ELSE 0
  END
```

在精确整数域，`count <= 0` 与 `count > 0` 覆盖整域且互斥，所以反转条件同时换两臂不改变结果。按[完整二分 CASE 选向](../../../../design/chapters/08-统一定形.md#cf-08-二分的比较基准)，唯一代表把正值放在 THEN、零放在 ELSE。把左、右操作数对换时也必须同步改比较符号。若阈值变为 `1`，当 `count = 1` 时结果改变，不能与本例合并。

## 0／1 结果不改变守卫的选向

```java
import normalizer.intrinsics.N;

class CanonicalCount {
    static int present(@N.Nullable String label) {
        if (label == null) return 0;
        return 1;
    }
}
```

**原样通过。** `label` 已取得，`== null` 和 `!= null` 互补，且两臂都是已经定义的数值。按同一比较基准选 `==`，不因为结果恰是 0／1 开特例。若把 1 改成 2，仍用同一方向，只改变非空分支的结果。该规则不重新求值标签。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalCount.java#CanonicalCount.present, returns: number }
P:
  parameters:
    label: { position: 0, type: 'text?' }
R: |-
  CASE
    WHEN label == null THEN 0
    ELSE 1
  END
```

另用 `n > 0 ? other : 0`、`n > 0 ? n : null` 与不同阈值对照，确保选向只规范守卫，不用分支返回值的内容反推条件或消去相异的结果。

若在已取得的精确值中按 `count > 0` 选择 7 或 11，`9 < 选中值` 恰在非正数域成立，而 `选中值 < 9` 恰在正数域成立。比较两侧不能只交换文本；两个常量对照都不新增 P、E 或 S。若另一侧是未知的复合式 `x + 1` 而不是一个叶值，不为了分配比较而把复合式复制到每条 CASE 臂。

## 纯返回和实际写入应用同一完整二分规范

在已取得的精确入参中，只有 (a, b) = (1, 2) 取 7，其余取 9。条件取反必须同时交换两臂；这既适用于纯返回的表达式，也适用于写入时已算出的值。后者仍然真实写一次状态，不读取入口已有值来补条件，不把写入合并进 R。预处理原样通过。

```java
import normalizer.intrinsics.N;
class GuardedResult {
    static final GuardedResult INSTANCE = new GuardedResult();
    @N.EntryState int result;
    static int pure(int a, int b) { return a == 1 && b == 2 ? 7 : 9; }
    int state(int a, int b) { result = a == 1 && b == 2 ? 7 : 9; return result; }
}
```

```yaml
schema: normalizer.pers/1
entry: { function: src/GuardedResult.java#GuardedResult.pure, returns: number }
P:
  parameters:
    a: { position: 0, type: number }
    b: { position: 1, type: number }
R: |-
  CASE
    WHEN a == 1 AND b == 2 THEN 7
    ELSE 9
  END
```

```yaml
schema: normalizer.pers/1
entry: { function: src/GuardedResult.java#GuardedResult.INSTANCE.state, returns: number }
P:
  parameters:
    a: { position: 0, type: number }
    b: { position: 1, type: number }
E:
  result: { kind: state, source: src/GuardedResult.java#GuardedResult.INSTANCE.result, type: number }
R: |-
  CASE
    WHEN a == 1 AND b == 2 THEN 7
    ELSE 9
  END
S:
  - write:
      target: result
      value: |-
        CASE
          WHEN a == 1 AND b == 2 THEN 7
          ELSE 9
        END
```
