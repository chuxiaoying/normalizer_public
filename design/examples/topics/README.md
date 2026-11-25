# 基础主题

基础主题收录一种可以独立建立语义合同、持续扩展案例并复用测试方法的能力。目录按测试要证明的主张归档，语言、业务背景和依赖能力通过种子元数据关联。

一个案例依赖另一个主题，不会因此自动成为联合主题。只有测试专门验证多个主题组合后新增的交互规则，才进入 `../compositions/`。

## 归档规则

- 一个种子只有一个主主题。
- `seed-manifest.yaml` 记录种子的 `kind`、主主题和关联主题，`seeds.yaml` 继续保存稳定 ID 到案例路径的权威映射。
- 主题必须有自己的语义合同、难度梯度或规模效应。单个基础防御案例先放入 `basic-contracts`。
- 主题页解释该能力要保留的业务语义，案例正文保留原始代码、AI 预处理结果和 PERS 结果。

## 当前主题

- [统一正则规范](unified-canonical-form/README.md)
- [表达式化简](expression-simplification/README.md)
- [集合](collections/README.md)
- [控制流](control-flow/README.md)
- [对象与类型](objects-and-types/README.md)
- [状态与别名](state-and-aliasing/README.md)
- [循环与终止](loops-and-termination/README.md)
- [外部边界](external-boundaries/README.md)
- [关系数据](relational-data/README.md)
- [数值域](number-domain/README.md)
- [语言入口](language-entries/README.md)
- [基础合同](basic-contracts/README.md)
