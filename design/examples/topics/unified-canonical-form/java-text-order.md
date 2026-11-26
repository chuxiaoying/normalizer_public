# 文本连接不交换：按顺序组成业务标签

## 设计意图

检验文本常量和转义先解释，连接保持原位置。移动前后缀可改变文本；字符字面量进入数值运算时遵循该类型，而不是强行按文本连接。

## 输入原始代码

标签按「类别、编号」组成。连接运算符虽然也写为 `+`，但不是精确整数加法。

```java
class CanonicalLabel {
    static String label() {
        return "zone-" + "42";
    }
}
```

## AI 预处理结果

**原样通过。** 两段都是原生文本常量。AI 不交换，不替内核折叠。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalLabel.java#CanonicalLabel.label, returns: text }
P: { parameters: {} }
R: '"zone-42"'
```

跳过排序不等于跳过常量执行。按源码顺序得到 `zone-42`；主动交换输入则得到 `42zone-`，不能复用同一黄金。两段分别连接空串不改变原结果。

## 字符常量先还原，才执行确切整数运算

```java
class CanonicalLabel {
    static int charCode() {
        return '\141' + '\n';
    }
}
```

**原样通过。** Java 的八进制转义 `\141` 是 UTF-16 单码元 97，换行是 10；相加是整数 107，而不是把源码的反斜线按文本拼接。用同一码元的普通字符替换转义，结果不变。

```yaml
schema: normalizer.pers/1
entry: { function: src/CanonicalLabel.java#CanonicalLabel.charCode, returns: number }
P: { parameters: {} }
R: '107'
```
