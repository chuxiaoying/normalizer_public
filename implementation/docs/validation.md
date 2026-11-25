# 构建与运行检查

在仓库根目录执行：

```sh
cargo check --locked --workspace --all-targets --manifest-path implementation/packages/normalizer/Cargo.toml
implementation/tools/normalizer help
implementation/tools/normalizer test business-examples
implementation/tools/normalizer test pers-goldens
implementation/tools/normalizer test seed-patches
python3 -B implementation/tools/docs/check_links.py
```

使用[入门示例](../../usage/getting-started.md)检查 CLI 的源码入口。构建和命令检查只覆盖相应入口的可运行性。从仓库根运行 `implementation/tools/normalizer test business-examples` 执行首批种子的 CLI 验收，覆盖范围与变体约定见[独立测试区](../../tests/README.md)。

## 采集实际覆盖

在空的临时目录中分别放构建产物、Rust profile 和 Node V8 原始报告，避免与日常构建或上一次采样混用。从仓库根目录执行：

```sh
evidence=$(mktemp -d)
mkdir "$evidence/rust" "$evidence/v8"
export CARGO_TARGET_DIR="$evidence/target"
export RUSTFLAGS="-C instrument-coverage"
export LLVM_PROFILE_FILE="$evidence/rust/%m-%p.profraw"
export NODE_V8_COVERAGE="$evidence/v8"
implementation/tools/normalizer test business-examples
implementation/tools/normalizer test pers-goldens
implementation/tools/normalizer test seed-patches
llvm-profdata merge -sparse "$evidence"/rust/*.profraw -o "$evidence/rust.profdata"
llvm-cov export -instr-profile="$evidence/rust.profdata" \
  "$CARGO_TARGET_DIR/debug/normalizer-normalized" > "$evidence/rust.json"
```

使用与本机 Rust 工具链 profile 格式兼容的 LLVM 工具。分析时只纳入仓库自有代码，区分实测零次、未插桩、平台代码及签字例外；不把导入、错误降级或覆盖率百分比当作测试证明。覆盖报告不进 Git。
