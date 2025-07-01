#!/bin/bash
# Vercel build script for fiddle-web-app
# This script:
# 1. Installs system dependencies needed for building
# 2. Installs Rust, Go, and pnpm directly
# 3. Builds the WASM components and fiddle-web-app
#
# Expected environment: Amazon Linux 2023 (Vercel build environment)

set -x
set -e

# Set locale to avoid warnings
export LC_ALL=C
export LANG=C

# Navigate to the root directory
cd ../../../

# Install system dependencies
echo "Installing system dependencies..."
# Install dependencies for compilation
dnf install -y gcc make readline-devel zlib-devel openssl-devel libyaml-devel
# Install dependencies for Rust/WASM compilation
dnf install -y llvm clang
# Install additional dependencies that might be needed
dnf install -y git wget tar gzip bzip2 xz
# Verify curl is available (either curl or curl-minimal)
if ! command -v curl &> /dev/null; then
    echo "Error: curl is not available"
    exit 1
fi

# Install Rust directly
echo "Installing Rust..."
if ! command -v rustc &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi

# Install Go directly
echo "Installing Go..."
if ! command -v go &> /dev/null; then
    GO_VERSION="1.23.0"
    wget -q "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz"
    tar -C /usr/local -xzf "go${GO_VERSION}.linux-amd64.tar.gz"
    rm "go${GO_VERSION}.linux-amd64.tar.gz"
    export PATH="/usr/local/go/bin:$PATH"
fi

# Ensure paths are set
export PATH="$HOME/.cargo/bin:/usr/local/go/bin:$PATH"

# Install Go tools
echo "Installing Go tools..."
# Install protoc-gen-go for protocol buffer generation
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
# Install goimports for Go import formatting
go install golang.org/x/tools/cmd/goimports@latest

# Ensure Go binaries are in PATH
export PATH="$HOME/go/bin:$PATH"

# Verify installations
echo "Rust version: $(rustc --version)"
echo "Go version: $(go version)"
echo "Node version: $(node --version)"
echo "pnpm version: $(pnpm --version)"

# Install required Rust tools
echo "Installing Rust tools..."
cargo install wasm-pack --version 0.13.1 || true
cargo install cross || true

# Add wasm target
rustup target add wasm32-unknown-unknown

# Now navigate to the baml-schema-wasm directory for building
cd engine/baml-schema-wasm
export OPENSSL_NO_VENDOR=1

# Go back to root directory
cd ../../

# Run the build
echo "Building fiddle-web-app..."
pnpm build:fiddle-web-app

ls -l
ls -l /vercel/output