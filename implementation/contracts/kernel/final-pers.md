# 最终业务 PERS

[输出 schema](../pers/schema.yaml)定义公开字段。[完整示例](../../../design/examples/)给出从原始源码、预处理输入到最终业务 PERS 的对照。[业务语义合同](../business-pers.md)解释数值域、空值、入口身份和来源。

`entry` 指明函数与返回类型，`P` 保留业务参数及位置，`E` 描述业务环境，`S` 按可观察顺序列出有条件的操作并关联完成值，`R` 描述正常返回，`throws` 声明异常类型和条件。纯计算可省略未使用的分区。例如 `HelloWorld.addOne` 输出 `R: value + 1`。
