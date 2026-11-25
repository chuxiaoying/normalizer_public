# 终态异常：按载荷排序，不按源码抛出先后排序

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
S: []
throws:
  - { type: java.lang.RuntimeException, when: 'code == 3' }
  - { type: java.lang.RuntimeException, when: 'code == 1', payload: '"alpha"' }
  - { type: java.lang.RuntimeException, when: 'code == 2', payload: '"zeta"' }
```

改变独立 guard 的源码顺序不改变这份结果；改变某个消息或引入重叠 guard 则会改变对应的异常结果。没有声明的消息不占据 payload 字段；用 `(java.lang.String) null` 显式选择 String 构造器并交付 null 则发布 `payload: 'null'`，不是遗漏字段。本切片仅测试直接 Java 的 RuntimeException 及其字面量消息，不替代外部端口载荷或任意异常对象的观察合同。带 cause 的构造器不得当作文本消息解释。

## 外部回执只在取得后进入异常 guard

```java
import normalizer.annotations.External;

class GuardedCanonicalFailure {
    @External
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

interface Classifier {
    int rank(int code);
}
```

**原样通过。** 端口只给出数值，异常和消息由当前 Java 方法决定。关闭入口开关时既没有 S 调用，也没有任何异常；交换两个独立 guard 时不重编号或复制回执。

```yaml
schema: normalizer.pers/1
entry: { function: src/GuardedCanonicalFailure.java#GuardedCanonicalFailure.validate, returns: number }
P:
  parameters:
    enabled: { position: 0, type: bool }
    code: { position: 1, type: number }
R: code
S:
  - call: { external: classifier.rank, args: [code], returns: number, result: '$e_0', when: enabled }
throws:
  - { type: java.lang.RuntimeException, when: 'enabled and $e_0 == 1', payload: '"alpha"' }
  - { type: java.lang.RuntimeException, when: 'enabled and $e_0 == 2', payload: '"zeta"' }
```
