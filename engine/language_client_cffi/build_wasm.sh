#!/bin/bash
set -e

echo "Building BAML FFI for WASM..."
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
wasm-pack build --target bundler --out-dir pkg --no-default-features --features wasm
echo "WASM build complete!"