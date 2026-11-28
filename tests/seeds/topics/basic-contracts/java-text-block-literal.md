# 原生文本块按 Java 字面量求值

## 设计意图

原生文字写法交给[符号执行](../../../../design/chapters/01-符号执行.md#se-02-执行覆盖与缺口)，不让 AI 将文本块改成引号字符串。按 [JLS §3.10.6](https://docs.oracle.com/javase/specs/jls/se25/html/jls-3.html#jls-3.10.6)，先处理物理换行，再去掉偶然缩进和行尾空白，最后解释转义。得到的是原有文本类型，不增加 PERS 机制。

## 输入与预处理

```java
class TextBlock {
    static String read() {
        return """
            first\s
              second\
            third
            """;
    }
}
```

**原样交付。** 第一行的转义空格保留，第二行保留两格有意义的缩进，续行转义连接第二、三行，最后仍有换行。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: {function: src/TextBlock.java#TextBlock.read, returns: text}
P: {parameters: {}}
R: '"first \n  secondthird\n"'
```

## 必须保留的差异

- 写成同内容的普通字符串，完整 PERS 不变。
- 结束定界符在内容行末时，不凭空附加终止换行。独立的结束定界符行参与确定去掉多少缩进。
- 物理 CRLF 是换行风格；源码显式转义出的 CR 是文本内容。非断行空格和孤立 UTF-16 码元不因去除缩进而消失。
- 缺少起始换行或转义不合法是源码错误，不能猜文本。
