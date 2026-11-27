# 构建与运行检查

在仓库根目录执行：

```sh
cargo check --locked --workspace --all-targets --manifest-path implementation/packages/normalizer/Cargo.toml
implementation/tools/normalizer help
python3 -B implementation/tools/docs/check_links.py
git diff --check
```

检查已暂存的改动时再跑 `git diff --cached --check`。架构指标及报告的判读见[架构健康](architecture-health.md)。用[入门示例](../../usage/getting-started.md)验真 CLI 用法；完整行为验收和定向复现命令集中在[测试区](../../tests/README.md)。

## 采集实际覆盖

先准备与 `rustc` 自带 LLVM 兼容的 `llvm-profdata` 和 `llvm-cov`（见 `rustc -vV`），确认二者在 PATH。以下在子 Shell 中采集，隔离临时构建、Rust 原始 profile 和 Node V8 报告；任一步失败则停止，不使用部分结果。

```sh
(
  set -e
  command -v llvm-profdata >/dev/null
  command -v llvm-cov >/dev/null
  evidence=$(mktemp -d)
  mkdir "$evidence/rust" "$evidence/v8"
  export CARGO_TARGET_DIR="$evidence/target"
  export RUSTFLAGS="-C instrument-coverage"
  export LLVM_PROFILE_FILE="$evidence/rust/%m-%p.profraw"
  export NODE_V8_COVERAGE="$evidence/v8"
  implementation/tools/normalizer test business-examples
  llvm-profdata merge -sparse "$evidence"/rust/*.profraw -o "$evidence/rust.profdata"
  llvm-cov export -instr-profile="$evidence/rust.profdata" \
    "$CARGO_TARGET_DIR/debug/normalizer-normalized" > "$evidence/rust.json"
  printf '覆盖结果：%s\n' "$evidence"
)
```

命令保存的是完整正式回归产生的原始覆盖证据。原始报告若没有分支计数，不能把区域命中当成分支已测；只使用同一提交、同一批正式用例的测量。覆盖分母、例外及保留规则见[仓库整理原则](repository-hygiene.md)。
