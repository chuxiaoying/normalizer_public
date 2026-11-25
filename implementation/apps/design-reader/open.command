#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ ! -d node_modules/markdown-it || ! -d node_modules/highlight.js ]]; then
  npm ci --ignore-scripts --no-audit --no-fund
fi
if [[ ! -f ../../tools/frontend/node_modules/yaml/dist/index.js ]]; then
  npm --prefix ../../tools/frontend ci --ignore-scripts --no-audit --no-fund
fi
exec node reader.mjs --open
