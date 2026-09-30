#!/usr/bin/env bash
set -euo pipefail

cargo build --locked --release --target wasm32-unknown-unknown -p web
wasm-bindgen \
  --target web \
  --out-dir public/assets/v1 \
  --out-name app \
  target/wasm32-unknown-unknown/release/web.wasm
