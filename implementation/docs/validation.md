# 构建与运行检查

在仓库根目录执行：

```sh
cargo check --locked --workspace --all-targets --manifest-path implementation/packages/normalizer/Cargo.toml
implementation/tools/normalizer help
python3 -B implementation/tools/docs/check_links.py
git diff --check
```

检查已暂存的改动时再跑 `git diff --cached --check`。架构指标及报告的判读见[架构健康](architecture-health.md)。用[入门示例](../../usage/getting-started.md)验真 CLI 用法；完整行为验收和定向复现命令集中在[测试区](../../tests/README.md)。

## Rust 格式门禁

新克隆后在仓库根目录执行一次（已有自定义 Git hooks 时，先把
`.githooks/pre-push` 接入现有 hook，再启用，不要覆盖已有检查）：

```sh
rustup toolchain install "$(cat implementation/tools/rustfmt-version)" --profile minimal --component rustfmt
git config --local core.hooksPath .githooks
```

日常运行 `implementation/tools/rust-format` 统一格式化所有受 Git 跟踪的 `.rs`；
新文件先执行 `git add <文件>`，格式化后重新暂存。只检查用
`implementation/tools/rust-format --check`。工具链版本和风格配置随仓库保存，
不受本机默认 Rust 工具链影响。

`git push` 前，hook 用固定版本的 rustfmt 检查每个待推送引用指向的提交中全部 Rust 文件。
它不读取未提交的文件，也不会修改工作区。检查失败时，格式化、提交修正后再推。
GitHub Actions 在 push 和 PR 上也运行同一检查；如果要挡住绕过本地 hook 的主干合并，
在 GitHub 将 `rustfmt` 设为主干必过检查。

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
