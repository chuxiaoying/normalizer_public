# 终态异常：按载荷排序，不按源码抛出先后排序

## 设计意图

`RuntimeException` 和 `java.lang.RuntimeException` 属于同一 Java 声明。不因限定名改变无载荷、文本载荷或显式文本 null 的规范结果；cause 构造器仍不是文本。

检验相同异常类型在不同载荷、有效域及实际回执下的唯一终态形态。可对已取得的结果按规范排序，不能倒排外部事件或跨成功／失败路径偷用载荷。

## 输入原始代码

校验返回原编号。编号 1 与编号 2 各有不同的异常消息，编号 3 仅声明运行时异常的类型。三种域互斥。

```java
class CanonicalFailure {
    static int validate(int code) {
        if (code == 2) throw new RuntimeException("zeta");
        if (code == 1) throw new RuntimeException("alpha");
        if (code == 3) throw new RuntimeException();
        return code;
    }
}
```

## AI 预处理结果

**原样通过。** 保留抛出的位置、类型、构造器选择与消息。AI 不把抛出改成默认返回，不将两个同类错误混为一个，也不预先交换分支。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalFailure.java#CanonicalFailure.validate, returns: number }
P: { parameters: { code: { position: 0, type: number } } }
R: code
throws:
  - { type: java.lang.RuntimeException, when: 'code == 3' }
  - { type: java.lang.RuntimeException, when: 'code == 1', payload: '"alpha"' }
  - { type: java.lang.RuntimeException, when: 'code == 2', payload: '"zeta"' }
```

[《PERS登场》PERS-05](../../../../design/chapters/07-PERS登场.md#pers-05-终态异常的展示顺序)先看异常类型，再看可选载荷，所以同类型的三项依次为无载荷、`"alpha"`、`"zeta"`。改变独立 guard 的源码顺序不改变这份结果；改变某个消息或引入重叠 guard 则会改变对应的异常结果。没有声明的消息不占据 payload 字段；用 `(java.lang.String) null` 显式选择 String 构造器并交付 null 则发布 `payload: 'null'`，不是遗漏字段。本切片仅测试直接 Java 的 RuntimeException 及其字面量消息，不替代外部端口载荷或任意异常对象的观察合同。带 cause 的构造器不得当作文本消息解释。

若只在编号 1、2、3 正常返回 10、20、30，其余编号抛出 `"invalid"`，执行层证明 `R` 只在这三个正常编号上有定义，才可以在 `R` 内把第三个值写成 `ELSE 30`。其余编号仍只走 `throws`；不能给通用部分 CASE 补一个默认返回值。

## 外部回执只在取得后进入异常 guard

```java
import normalizer.intrinsics.N;

class GuardedCanonicalFailure {
    static final GuardedCanonicalFailure INSTANCE = new GuardedCanonicalFailure(new Classifier());

    @N.External
    private final Classifier classifier;

    GuardedCanonicalFailure(Classifier classifier) {
        this.classifier = classifier;
    }

    int validate(boolean enabled, int code) {
        if (enabled) {
            int rank = classifier.rank(code);
            if (rank == 2) throw new RuntimeException("zeta");
            if (rank == 1) throw new RuntimeException("alpha");
        }
        return code;
    }
}

class Classifier {
    native int rank(int code);
}
```

**原样通过。** 端口只给出数值，异常和消息由当前 Java 方法决定。关闭入口开关时既没有 S 调用，也没有任何异常；交换两个独立 guard 时不重编号或复制回执。

```yaml
schema: normalizer.pers/1
entry: { function: src/GuardedCanonicalFailure.java#GuardedCanonicalFailure.INSTANCE.validate, returns: number }
P:
  parameters:
    enabled: { position: 0, type: bool }
    code: { position: 1, type: number }
R: code
S:
  - call: { external: src/GuardedCanonicalFailure.java#GuardedCanonicalFailure.INSTANCE.classifier.rank, args: [code], returns: number, result: '$e_0', when: enabled }
throws:
  - { type: java.lang.RuntimeException, when: 'CASE WHEN enabled THEN $e_0 == 1 ELSE false END', payload: '"alpha"' }
  - { type: java.lang.RuntimeException, when: 'CASE WHEN enabled THEN $e_0 == 2 ELSE false END', payload: '"zeta"' }
```

## 外部异常不能借另一条回执提供载荷

外部异常若有业务可观察的载荷，按 [PERS-03](../../../../design/chapters/07-PERS登场.md#pers-03-完成与完整等价) 和 [PERS-07](../../../../design/chapters/07-PERS登场.md#pers-07-完成的互补只在已取得域内成立)，只能从那次调用真实取得的对应异常完成观察；不能从普通 Java 字段名猜测外部载荷合同。本例不另定载荷声明或投影语法。
