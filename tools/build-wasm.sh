#!/usr/bin/env bash
# Compiles the Rust engine to WebAssembly and embeds it in js/holdem-wasm.js.
# Needs: Rust (rustup) with the wasm target, and Node.js.
#   rustup target add wasm32-unknown-unknown
#   bash tools/build-wasm.sh
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --target wasm32-unknown-unknown --manifest-path engine/Cargo.toml
node tools/embed-wasm.js engine/target/wasm32-unknown-unknown/release/holdem_core.wasm js/holdem-wasm.js
