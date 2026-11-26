# 在线状态

局部变量保存已求值的符号表达式。对象保存身份和字段，字段写入记录生效的路径；有条件的读取只需要在该读取可达的路径上得到已定义的值。入口参数、宿主状态、局部对象和外部响应各有来源。相同字段名不会把它们合并成同一个对象。

- [分支、堆和完成](realtime-normalization.md)描述执行器何时合并和如何投影。
- [Java 对象出口](../../design/examples/topics/objects-and-types/java-static-object.md)演示构造器执行及返回内容。
- [Vue 事件](../../design/examples/topics/language-entries/frontend-vo-production-vue.md)演示修改宿主状态、等待外部返回及后续写入。
- [业务 PERS](business-pers.md)定义公开的值域和来源。
