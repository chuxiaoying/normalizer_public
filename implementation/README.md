# 实现

业务规则查[设计区](../design/README.md)，公开用法查[使用区](../usage/README.md)，正式验收查[测试区](../tests/README.md)。

| 工作 | 入口 |
|---|---|
| 改预处理与语言接线 | [三模块接线](docs/architecture-three-modules.md) · [预处理 Skill（使用区）](../usage/skills/normalizer-preprocess/SKILL.md) · [语言适配](tools/frontend/README.md) |
| 改内部交接或 PERS 发布 | [规范源码交接](contracts/normalized-source-contract.md) · [PERS 机器接口](contracts/business-pers.md) |
| 找责任层、排查错误或性能 | [维护路由](docs/agent-maintenance.md) · [内核代码地图](docs/kernel-execution.md) · [诊断与性能](docs/troubleshooting.md) |
| 开发、验证并交付 | [贡献指南](docs/contributing.md) · [实现原则](docs/principles.md) · [构建和覆盖](docs/validation.md) · [发布合同](contracts/kernel/release.md) |
| 清理仓库及维护工具 | [整理准入](docs/repository-hygiene.md) · [架构指标](docs/architecture-health.md) · [设计阅览器](apps/design-reader/README.md) |
