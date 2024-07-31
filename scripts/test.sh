#!/bin/bash

# Stop on error:
set -e

cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-targets --workspace
cargo clippy --all-targets --all-features --workspace
cargo test --workspace
cargo test --all-features --workspace
wasm-pack test --node leptos_workers
WASM_BINDGEN_TEST_TIMEOUT=100 wasm-pack test --firefox --headless leptos_workers
