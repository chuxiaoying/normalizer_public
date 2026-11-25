#!/usr/bin/env bash
# Run from the repository root or package directory. Git resolves this relative
# hooksPath in each working tree, so tracked hook updates take effect immediately.
set -euo pipefail
repo_root="$(git rev-parse --show-toplevel)"
hooks_path="implementation/packages/normalizer/scripts/git-hooks"
if [[ ! -x "$repo_root/$hooks_path/pre-commit" ]]; then
    echo "[hooks] 缺少可执行 hook：$hooks_path/pre-commit" >&2
    exit 1
fi
git config --local core.hooksPath "$hooks_path"
# A worktree-specific setting takes precedence over the shared local setting.
if [[ "$(git config --local --get extensions.worktreeConfig || true)" == true ]]; then
    git config --worktree core.hooksPath "$hooks_path"
fi
echo "[hooks] 已使用版本管理的 ${hooks_path}，提交前检查暂存差异。"
