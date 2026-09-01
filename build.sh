#!/bin/bash
# 使用国内 npm 镜像构建 File Tools（Tauri）
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v rustc &>/dev/null; then
  echo "[ERROR] Rust not found. Install: https://rustup.rs"
  exit 1
fi

if ! command -v npm &>/dev/null; then
  echo "[ERROR] npm not found."
  exit 1
fi

echo "[1/2] Installing npm dependencies..."
npm install --registry=https://registry.npmmirror.com

echo "[2/2] Building..."
npm run build

echo ""
echo "Done. Bundle: src-tauri/target/release/bundle/"
