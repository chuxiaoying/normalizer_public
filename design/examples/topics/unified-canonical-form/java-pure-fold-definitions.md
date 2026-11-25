# 两个纯有序递推的稳定 T 定义

## 输入原始代码

给定两个已取得的非空引用数组，元素是精确整数（数组允许长度为零）。从相同初值出发逐项更新，再求两个终值之差。

```java
class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir
           leftValue = reduce left from initial as (leftAcc, leftItem) yield leftAcc * 2 + leftItem
        */
        int rightValue;
        /* @normalizer-ir
           rightValue = reduce right from initial as (rightAcc, rightItem) yield rightAcc * 2 + rightItem
        */
        return leftValue - rightValue;
    }
}
```

## AI 预处理结果

**原样通过。** 局部声明已经明确初值、元素、次序和更新公式。不能改成按样本长度展开，不能把纯定义改为外部事件。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalFolds.java#CanonicalFolds.difference, returns: number }
P:
  parameters:
    left: { position: 0, type: 'number[]' }
    right: { position: 1, type: 'number[]' }
    initial: { position: 2, type: number }
T:
  - { id: $t_0, op: fold, type: number, source: left, initial: initial, accumulator: acc, element: item, body: 'acc * 2 + item' }
  - { id: $t_1, op: fold, type: number, source: right, initial: initial, accumulator: acc, element: item, body: 'acc * 2 + item' }
R: $t_0 - $t_1
```

两个定义均无依赖，按公开输入 left、right 排列。acc 和 item 只在定义体内有效；不泄漏源码的私有变量名。初值为 1、左侧 [3,4] 时左值为 14；换为 [4,3] 则为 15，证明不能重排元素。若左递推以右递推终值为初值，右定义必须先发布，且左定义、R 一并使用新编号。
