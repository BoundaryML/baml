#!/bin/bash
set -x
set -e

# Install LLVM and Clang
LLVM_VERSION=12  # Specify the version you need
CLANG_VERSION=12  # Specify the version you need

echo "Installing LLVM version $LLVM_VERSION and Clang version $CLANG_VERSION"

# Update package list and install LLVM and Clang
apt-get update
apt-get install -y llvm-$LLVM_VERSION clang-$CLANG_VERSION

# Optionally, set environment variables or update PATH
LLVM_PATH=/usr/lib/llvm-$LLVM_VERSION
export PATH=$LLVM_PATH/bin:$PATH

echo "LLVM and Clang installed successfully"

# Existing script
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
export PATH="/vercel/.cargo/bin:$PATH"

source $HOME/.cargo/env

which cargo
# clang --version
#llvm-config --version
# g++ --version

dnf install -y llvm
dnf install -y clang

cd ../../engine/baml-schema-wasm
export OPENSSL_NO_VENDOR=1
# cargo install
rustup target add wasm32-unknown-unknown
cargo update -p wasm-bindgen
cargo install -f wasm-bindgen-cli@0.2.92

# cargo build
cd ../../typescript/fiddle-frontend

npm add -g tsup

echo "Path: $(pwd)"

turbo build --filter=fiddle-frontend
echo "Path2: $(pwd)"

ls -l
ls -l /vercel/output
