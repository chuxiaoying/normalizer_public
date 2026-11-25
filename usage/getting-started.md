# 把一个 Java 方法归一化为 PERS

从仓库根目录运行下面的示例。Java 代码已经闭合，可以直接作为预处理输入。

```sh
project=$(mktemp -d)
mkdir "$project/src"
cat > "$project/src/HelloWorld.java" <<'JAVA'
class HelloWorld {
    static int addOne(int value) { return value + 1; }
}
JAVA
implementation/tools/normalizer source "$project" HelloWorld.addOne
```

在输出中，`entry.function` 指向 `src/HelloWorld.java#HelloWorld.addOne`，`P.parameters.value` 的类型是 `number`，`R` 为 `value + 1`。工具把最终 YAML 写到 stdout，输入无法闭合时返回非零并将诊断写到 stderr。

遇到业务 helper，应保留可执行源码；遇到真实外部服务，应交付端口声明；遇到查询或外部写入，可用局部 IR 标记补充预处理语义。对照[示例目录](../design/examples/)查看原始源码、预处理差异和完整输出。其他入口见 [CLI 参考](cli.md)。
