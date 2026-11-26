# Hello World：嵌套记录字段投影

## 设计意图

字段声明中的 `Object` 和 `java.lang.Object` 都是同一个不透明引用类型，不能按拼写分裂；读取子字段仍须有源码声明的子对象布局，不能依靠不透明引用猜字段。

检验嵌套字段投影保留真实对象层级。改访问链上一段应改变引用路径；同名末端字段不能让两个不同的父对象被合并。各层局部别名不缩短借入路径；两个子对象的同名字段独立参与非交换差值。

用类名限定调用有定义的静态辅助方法时，形参名不是新来源。传整条记录，或先取得内层字段再传给第二个辅助方法，都仍观察 `outer.inner.value`，不制造外部调用。

## 输入原始代码

入口接收一个包含嵌套记录的对象，只读取最内层的业务字段。

```java
class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int read(Outer outer) {
        return outer.inner.value;
    }
}
```

## AI 预处理结果

**原样通过。** 这是合法的原生 Java。AI 不把 `inner` 或 `value` 提升为新的入口参数，也不把嵌套对象改写成扁平 map。

**预处理要点**：对象类型和字段来源交给统一 IR 建立，预处理不为嵌套字段制造额外输入或隐藏对象结构。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Inner:
    fields:
      value: number
  Outer:
    fields:
      inner: Inner

entry:
  function: src/Nested.java#Nested.read
  returns: number

P:
  parameters:
    outer:
      position: 0
      type: Outer

R: outer.inner.value
```

**PERS 验收要点**：嵌套对象的类型路径是业务来源的一部分。PERS 需要完整声明中间记录，不能只凭最终字段名猜测来源。
