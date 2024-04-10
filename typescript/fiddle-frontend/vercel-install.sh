#!/bin/bash
set -x
set -e
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
export PATH="/vercel/.cargo/bin:$PATH"

rustup target add wasm32-unknown-unknown
cargo update -p wasm-bindgen
cargo install -f wasm-bindgen-cli@0.2.87
pnpm i