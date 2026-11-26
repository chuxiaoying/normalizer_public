# Hello World：业务流水号的显式回卷

## 设计意图

区分连续业务整数与源码明确写出的回卷规则。达到阈值才回到约定起点，未达到继续加；不从 Java 载体的位宽推断额外回卷。

[本主题](README.md)

## 输入原始代码

业务协议规定：流水号到 2147483647 后，下一个编号从 −2147483648 开始。源码把协议边界写成条件。

```java
class Sequence {
    static int next(int current) {
        if (current == 2147483647) return -2147483648;
        return current + 1;
    }
}
```

## AI 预处理结果

**原样通过。** 保留比较、常量和两个 return。解析器交付显式的协议条件，内核按业务 number 合并返回路径。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry: { function: src/Sequence.java#Sequence.next, returns: number }
P: { parameters: { current: { position: 0, type: number } } }
R: |-
  CASE
    WHEN current == 2147483647 THEN -2147483648
    ELSE current + 1
  END
```

**判别例**　2147483646 的后继是 2147483647；到达 2147483647 后回到 −2147483648。若删除源码中的 if，理想结果就回到基础例的 current + 1；两份 Java 在 int 域内结果相同，两份业务设计表达的规则不同。[无显式回卷的加法](../expression-simplification/java-static.md)
