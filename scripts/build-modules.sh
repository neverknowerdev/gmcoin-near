#!/usr/bin/env bash
# Builds the OutLayer modules (WASI P2) into res/ and prints their sha256,
# which is what OutLayer secrets are bound to and core's set_modules stores.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p res
(cd modules && cargo build --release --target wasm32-wasip2 >/dev/null 2>&1)
for m in x-verify x-scan; do
  cp "modules/target/wasm32-wasip2/release/$m.wasm" "res/$m.wasm"
done
sha256sum res/x-verify.wasm res/x-scan.wasm
