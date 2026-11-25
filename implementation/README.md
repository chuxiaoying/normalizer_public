# 实现

| 任务 | 位置 |
|---|---|
| 浏览设计案例和架构指导 | [本地 HTML 阅览器](apps/design-reader/README.md) |
| 准备规范源码和边界材料 | [AI 预处理 Skill](skills/normalizer-preprocess/SKILL.md) |
| 调用用户入口或分派场景 | [CLI](tools/normalizer) · [场景适配器](tools/scenarios/) |
| 修改 JS 和 Vue 的语言适配 | [正式入口的内部适配](tools/frontend/README.md) |
| 修改 Java 解析、符号执行、数学化简或输出 | [Rust workspace](packages/normalizer/Cargo.toml) |
| 查阅 PERS、统一正则规范和模块交接细则 | [PERS schema](contracts/pers/schema.yaml) · [统一正则规范](../design/unified-canonical-form.md) · [三模块职责](docs/architecture-three-modules.md) |
| 检查仓库结构和文档链接 | [架构检查](tools/architecture/) · [文档检查](tools/docs/) |

从仓库根目录检查构建：

```sh
cargo check --locked --workspace --all-targets --manifest-path implementation/packages/normalizer/Cargo.toml
implementation/tools/normalizer help
```

业务语义由[设计区](../design/README.md)定义。从种子派生的 CLI 验收见[独立测试区](../tests/README.md)。
