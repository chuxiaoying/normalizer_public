# Hello World：Java 静态函数

## 设计意图

取最小闭合静态入口，区分源码整数 token 与无界业务加法。已知常量先求值，入口参数仍保持符号；改变常量一与二应改变 R，不改变入口参数定义。

## 输入原始代码

这是最小的 Java 静态函数案例。函数只读取一个 `int` 参数并返回加一后的值，没有状态写入、外部边界调用、环境读取或异常路径。

```java
class HelloWorld {
    static int addOne(int value) {
        return value + 1;
    }
}
```

## AI 预处理结果

**原样通过。** 入口只读取参数并执行一次整数加法。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/HelloWorld.java#HelloWorld.addOne
  returns: number

P:
  parameters:
    value:
      position: 0
      type: number

R: value + 1
```

本例只观察参数 `value` 和返回值 `value + 1`。

**PERS 验收要点**

- 用无固定大小上限的业务 `number` 统一数值计算，让业务规则摆脱宿主语言的位宽与回绕。
- 最终产物应自行交代“正在解释哪个入口、值从哪里来”，不依赖读者从命令或源码猜测符号含义。
- 结构只承载本例实际存在的信息，纯计算的结果无需被空分区包围。
