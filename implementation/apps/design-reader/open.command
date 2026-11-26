#!/bin/bash
set -euo pipefail
CDPATH= cd -- "$(dirname "$0")"

# Finder can launch Terminal without a shell profile that adds Homebrew.
if [[ "$(uname -s)" == Darwin ]]; then
  export PATH="$PATH:/opt/homebrew/bin:/usr/local/bin"
fi
if ! command -v node >/dev/null || ! command -v npm >/dev/null ||
   ! node -e 'process.exit(Number(process.versions.node.split(".")[0]) < 24 ? 1 : 0)'; then
  printf '%s\n' '设计区阅览器需要 Node.js 24 或更高版本和 npm。安装后请重新打开此脚本。' >&2
  exit 1
fi

if [[ ! -d node_modules/markdown-it || ! -d node_modules/highlight.js ]]; then
  npm ci --ignore-scripts --no-audit --no-fund
fi
if [[ ! -f ../../tools/frontend/node_modules/yaml/dist/index.js ]]; then
  npm --prefix ../../tools/frontend ci --ignore-scripts --no-audit --no-fund
fi
exec node reader.mjs --open
