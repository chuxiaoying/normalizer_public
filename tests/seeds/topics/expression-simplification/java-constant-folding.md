# Hello World：恒等加法折叠

## 设计意图

区分一个恒定子式与未知输入。加零、已知两端应先化成确定结果；改变常量或把一端换成参数时，不许继续复用原常量答案。

## 输入原始代码

入口把输入加上零后返回。这个案例只观察普通数学恒等式的化简。

```java
class Constant {
    static int normalize(int value) {
        return value + 0;
    }
}
```

## AI 预处理结果

**原样通过。** 代码是 Java 原生表达式，化简交给 normalizer 内核。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Constant.java#Constant.normalize
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number

R: value
```

**预处理要点**：预处理不替代内核做可计算的数学化简，能由工程处理的表达式保留原状。

**PERS 验收要点**：归一化结果只保留业务有效关系，消除恒等项以建立唯一、可断言的表达式形式。
