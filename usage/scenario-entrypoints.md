# 选择正式入口

只登记[正式 CLI 回归](../tests/README.md)实际执行的路径。AI 按[设计种子](../implementation/skills/normalizer-preprocess/references/seed-coverage.md)交付已闭合输入，不依赖专用预处理工具。

| 已准备的输入 | 命令 |
|---|---|
| Java 源目录，静态或实例业务方法 | `source SOURCE_DIR 'src/Entry.java#Entry.run'` |
| 严格 TypeScript 标量模块 | `module PROJECT_ROOT --entry 'src/module.ts#exportedFunction'` |
| 独立 TypeScript 局部元组模块 | `module PROJECT_ROOT --entry 'src/receipt.ts#traceCount'` |
| 独立 TypeScript 同步监听模块 | `module PROJECT_ROOT --entry 'src/handler.ts#handle'` |
| Vue 原组件和属性观察模块 | `scenario frontend-vo-consumption PROJECT_ROOT --entry 'src/page.ts#render' --source src/page.vue` |
| Vue 原组件和事件模块 | `scenario frontend-vo-production PROJECT_ROOT --entry 'src/page.ts#onLoad' --source src/page.vue` |

命令从仓库根运行，前缀为 `implementation/tools/normalizer`。参数和退出行为见[CLI 参考](cli.md)。
