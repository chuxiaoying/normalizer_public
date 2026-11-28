# Hello World：业务数值边界比较

## 设计意图

对照大整数、相邻整数和相等点的严格／非严格比较。操作数交换必须同步翻转比较符；不能借宿主溢出决定合法业务数值的大小。

## 输入原始代码

入口判断业务数值是否达到零边界。Java 的 `int` 在这里只承载业务 number，不引入宿主位宽语义。

```java
class Bound {
    static boolean safe(int value) {
        return value >= 0;
    }
}
```

## AI 预处理结果

**原样通过。** 这是 Java 原生比较，数值域由统一业务合同解释。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/Bound.java#Bound.safe
  returns: bool

P:
  parameters:
    value:
      position: 0
      type: number

R: value > -1
```

**预处理要点**：预处理只补充原生源码没有表达清楚的合同，不把业务 number 改写成 Java 位宽算法。

**PERS 验收要点**：PERS 关注业务关系，语言的存储宽度和溢出细节不进入无界业务 number 的内核语义。

**不可达域对照**　将方法体改为 `if (value >= 0) return value < 0 ? value == 0 : true; return false;`。进入内层真臂需要同时满足 `value >= 0` 和 `value < 0`，没有这种输入，不为该臂生成真假值；外层分别返回 true、false，完整预期仍是 `R: value > -1`。
