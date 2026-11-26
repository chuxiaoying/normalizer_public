# Hello World：对象返回的字段结构

## 设计意图

检验有源码构造的对象如何发布字段结构和字段来源。交换声明或私有名不改结构语义；加入初始化、改字段或构造委托需按真实源码合同另判。

## 输入原始代码

入口构造一个包含两个业务字段的对象并返回。对象类型和字段关系是返回合同的一部分。

```java
class Pair {
    int left;
    int right;

    Pair(int left, int right) {
        this.left = left;
        this.right = right;
    }
}

class PairFactory {
    static Pair make(int value) {
        return new Pair(value, value + 1);
    }
}
```

## AI 预处理结果

**原样通过。** 直接构造对象和 `this.field` 赋值属于当前源码合同。显式 `this(...)` 或 `super(...)` 委托有自己的初始化顺序（[JLS 8.8.7.1](https://docs.oracle.com/javase/specs/jls/se21/html/jls-8.html#jls-8.8.7.1)），不能被当作普通方法或直接删除；当前源码入口在委托关键字提示先预处理，不改变本例对 `this.field` 的支持。 字段声明自带的初始化式也不能被忽略，即使构造器稍后会重写同一字段；当前合同要求在支持的构造器源码内表达初始化。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Pair:
    fields:
      left: number
      right: number

entry:
  function: src/PairFactory.java#PairFactory.make
  returns: Pair

P:
  parameters:
    value:
      position: 0
      type: number

R: |-
  {
    left: value,
    right: value + 1
  }
```

**预处理要点**：能被语言 parser 闭合的对象结构不需要 AI 另造 record 语法。

**PERS 验收要点**：自定义类型由 `types` 统一定义，PERS 在返回处表达业务字段值，避免无根的宿主类型名。
