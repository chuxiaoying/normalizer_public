# 联合主题

联合主题只收录专门验证多个基础主题交互规则的种子。基础主题分别成立时，不能推出组合后的结果，才需要联合主题。

联合种子必须写清三个关系：

- `topics` 列出参与的基础主题。
- `claim` 说明组合后新增的业务规则。
- `kind: composition` 区分它和单主题种子。

只是在循环中处理集合、在 SQL 结果上做聚合，或者在 Vue 入口中更新状态，都不会自动成为联合主题。测试重点仍由主主题承担时，种子归入主主题，其他能力记录为关联主题。

## 当前预留方向

- [循环与集合](loops-and-collections/README.md)
- [关系数据与集合](relational-data-and-collections/README.md)
- [集合与表达式化简](collections-and-expression-simplification/README.md)
