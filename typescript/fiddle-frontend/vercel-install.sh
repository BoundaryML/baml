#!/bin/bash
set -x
set -e
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
export PATH="/vercel/.cargo/bin:$PATH"

# source $HOME/.cargo/env

which cargo
cd ../../engine/baml-schema-wasm
# cargo install
rustup target add wasm32-unknown-unknown
cargo update -p wasm-bindgen
cargo install -f wasm-bindgen-cli@0.2.87

cargo build
cd ../../typescript/fiddle-frontend
turbo build --filter=@baml/prompt-fiddle-next