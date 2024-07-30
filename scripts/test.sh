#!/bin/bash

# Stop on error:
set -e

cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --workspace
cargo test --all-features --workspace
wasm-pack test --node leptos_workers
wasm-pack test --chrome leptos_workers
