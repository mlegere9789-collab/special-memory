#!/usr/bin/env bash
# Build the WebAssembly debug viewer into shells/viewer-web/www/pkg (not committed).
# Needs: rustup target wasm32-unknown-unknown, and wasm-bindgen CLI version 0.2.130
# (cargo install wasm-bindgen-cli --version =0.2.130 --locked).
set -euo pipefail
cd "$(dirname "$0")/.."
WB="${WASM_BINDGEN:-wasm-bindgen}"
cargo build --release --locked -p viewer-web --target wasm32-unknown-unknown
"$WB" --target web --out-dir shells/viewer-web/www/pkg --out-name viewer_web \
  target/wasm32-unknown-unknown/release/viewer_web.wasm
ls -l shells/viewer-web/www/pkg/viewer_web_bg.wasm
