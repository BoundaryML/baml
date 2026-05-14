#!/bin/bash
# Vercel build script for app-promptfiddle (typescript2 workspace).
#
# Modeled on typescript/apps/fiddle-web-app/vercel-build.sh, but simpler:
#   - No Go toolchain (pkg-proto uses the `buf` npm package, not protoc-gen-go)
#   - Single Rust crate to build: baml_language/crates/bridge_wasm
#
# Expected environment: Amazon Linux 2023 (Vercel build environment).
# Run from Vercel Root Directory = typescript2/app-promptfiddle.

set -x
set -e

export LC_ALL=C
export LANG=C

# Move up to the typescript2 workspace root.
cd ..

# --- System deps for Rust/WASM compilation -----------------------------------
echo "Installing system dependencies..."
dnf install -y gcc make readline-devel zlib-devel openssl-devel libyaml-devel
dnf install -y llvm clang
dnf install -y git wget tar gzip bzip2 xz

if ! command -v curl &> /dev/null; then
    echo "Error: curl is not available"
    exit 1
fi

# --- Rust toolchain ----------------------------------------------------------
echo "Installing Rust..."
if ! command -v rustc &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"

# rustup shims may exist without a default toolchain set
rustup default stable
rustup target add wasm32-unknown-unknown

echo "Installing wasm-pack..."
cargo install wasm-pack --version 0.14.0 || true

# --- Verify --------------------------------------------------------------------
echo "Rust version: $(rustc --version)"
echo "Node version: $(node --version)"
echo "pnpm version: $(pnpm --version)"
echo "wasm-pack version: $(wasm-pack --version)"

# --- Install workspace deps + build dependencies -----------------------------
export OPENSSL_NO_VENDOR=1

echo "Installing pnpm workspace..."
# Vercel sets NODE_ENV=production, which makes pnpm skip devDependencies
# (buf, typescript, etc.). Force-install them since we need them to build.
pnpm install --frozen-lockfile --prod=false

echo "Generating proto types..."
pnpm --filter @b/pkg-proto run generate

echo "Building bridge_wasm..."
pnpm --filter @b/pkg-playground run build:wasm

# --- Build the Next.js app ---------------------------------------------------
echo "Building app-promptfiddle..."
pnpm --filter app-promptfiddle run build

ls -l app-promptfiddle/.next-build || true
