#!/usr/bin/env bash
# Builds every contract WASM into res/.
#   res/gm_token.wasm          token
#   res/gm_core.wasm           core, mainnet build (no staging methods)
#   res/gm_core_staging.wasm   core with the `staging` feature (advance_clock)
#   res/mock_outlayer.wasm     test double of OutLayer
# Pass --reproducible to build token and core with `cargo near build reproducible-wasm` (needs Docker).
set -euo pipefail
cd "$(dirname "$0")/.."
mode=non-reproducible-wasm
[[ "${1:-}" == "--reproducible" ]] && mode=reproducible-wasm
mkdir -p res

(cd contracts/token && cargo near build $mode --no-abi >/dev/null 2>&1)
cp target/near/gm_token/gm_token.wasm res/gm_token.wasm

(cd contracts/core && cargo near build non-reproducible-wasm --no-abi --features staging >/dev/null 2>&1)
cp target/near/gm_core/gm_core.wasm res/gm_core_staging.wasm
(cd contracts/core && cargo near build $mode --no-abi >/dev/null 2>&1)
cp target/near/gm_core/gm_core.wasm res/gm_core.wasm

(cd contracts/mock-outlayer && cargo near build non-reproducible-wasm --no-abi >/dev/null 2>&1)
cp target/near/mock_outlayer/mock_outlayer.wasm res/mock_outlayer.wasm

# A staging-only method must never ship in the mainnet build.
if grep -q advance_clock res/gm_core.wasm; then
  echo "error: res/gm_core.wasm exports a staging method" >&2
  exit 1
fi
sha256sum res/*.wasm
