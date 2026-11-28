#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
if [[ "${1:-}" == --build-only ]]; then build_only=1; shift; else build_only=0; fi
if [[ ${PINE_SKIP_NPM_CI:-0} != 1 ]]; then (cd frontend && npm ci --no-audit --no-fund); fi
(cd frontend && npm run build)
mkdir -p backend/build
if command -v javac >/dev/null; then compiler=(javac --release 21); else compiler=(java -m jdk.compiler/com.sun.tools.javac.Main -source 21 -target 21); fi
"${compiler[@]}" --add-modules jdk.httpserver -d backend/build backend/src/practice/chat/*.java
if [[ $build_only == 0 ]]; then
    exec java --add-modules jdk.httpserver -cp backend/build practice.chat.ChatServer "${1:-8787}" frontend/dist
fi
