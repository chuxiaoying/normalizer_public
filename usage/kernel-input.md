# 规范输入

从 Java 源目录开始时，提交已闭合的入口与依赖源文件，并给出 `Class.method`。参数及返回类型从声明读取，端口和查询需要真实的来源与语义。最小运行示例见[入门](getting-started.md)。

语言适配器也可生成 `normalizer.kernel-input/1` 工程。工程中 `source` 指向签名载体，`source_text` 保存对应内容，`entries` 列出入口，`functions` 为所需的无实现方法提供带来源的类型化定义。`entry_contexts` 按入口声明原业务函数、返回类型、真实形参和环境。中间载体的名字通过上下文映射回业务来源。

读取宿主输入和写入状态只依据当前所选入口的 P／E 声明。若入口缺少上下文、字段或参数，内核报错，不从同一工程的其他入口借用同名声明。

Vue 的投影与事件输入可在[生产示例](../design/examples/series-1/frontend-vo-production-vue.md)及[消费示例](../design/examples/series-1/frontend-vo-consumption-vue.md)中对照。术语和输出字段遵循[业务 PERS](../implementation/contracts/business-pers.md)。
