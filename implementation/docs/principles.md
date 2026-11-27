# Normalizer 实现原则

业务语义由[八章一级设计](../../design/README.md)裁决，二级专题细化；实现文档不能覆盖它们。输入、输出和规则自身的验收分别见[源码交接](../contracts/normalized-source-contract.md)、[机器接口](../contracts/business-pers.md)与[正式测试](../../tests/README.md)。

## 改动与诊断

1. 先固定基线、最小源码、入口、命令和失败阶段。按[维护路由](agent-maintenance.md)判断是输入绑定、执行、化简还是发布，勿用 formatter 修复执行错误。
2. 归约同时核对具体值与符号值。先用源码中的具体常量复现原语义，再用未知输入检验定义域与表达式；具体常量仍交给内核执行。按[内核代码地图](kernel-execution.md)定位负责状态与效果的代码。
3. 定形分歧回到[统一定形](../../design/chapters/08-统一定形.md)：从独立源码得到唯一预期，核对有效域和复杂度；不把 actual 回填为 expected。改动要用相同入口复现，并分别核对副作用、完成域与结构。

## 验收分层

Java、TypeScript 和 Vue 的端到端用例都由[正式回归](../../tests/README.md)执行；语言适配用例聚焦各自的语法、类型和接线，不重复整个内核矩阵。PERS 结构校验在回归内执行；构建、文档和性能检查见[验证入口](validation.md)，不能替代业务回归。更一般的提交和清理约定见[全仓约定](../../AGENTS.md)及[仓库整理原则](repository-hygiene.md)。
