#!/bin/bash
set -e

echo "🚀 Setting up BAML development environment..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo is not installed. Please install Rust first:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if pnpm is installed
if ! command -v pnpm &> /dev/null; then
    echo "❌ pnpm is not installed. Please install pnpm first:"
    echo "   npm install -g pnpm"
    exit 1
fi

# Install cargo-watch if not already installed
if ! command -v cargo-watch &> /dev/null; then
    echo -e "${YELLOW}📦 Installing cargo-watch for Rust hot reloading...${NC}"
    cargo install cargo-watch
    echo -e "${GREEN}✅ cargo-watch installed${NC}"
else
    echo -e "${GREEN}✅ cargo-watch already installed${NC}"
fi

# Install wasm-bindgen-cli if not already installed (needed for WASM builds)
if ! command -v wasm-bindgen &> /dev/null; then
    echo -e "${YELLOW}📦 Installing wasm-bindgen-cli...${NC}"
    cargo install wasm-bindgen-cli --version 0.2.92
    echo -e "${GREEN}✅ wasm-bindgen-cli installed${NC}"
else
    echo -e "${GREEN}✅ wasm-bindgen-cli already installed${NC}"
fi

# Install wasm-pack if not already installed (needed for building Rust WASM packages)
if ! command -v wasm-pack &> /dev/null; then
    echo -e "${YELLOW}📦 Installing wasm-pack...${NC}"
    cargo install wasm-pack
    echo -e "${GREEN}✅ wasm-pack installed${NC}"
else
    echo -e "${GREEN}✅ wasm-pack already installed${NC}"
fi

echo ""
echo -e "${GREEN}🎉 Development environment setup complete!${NC}"
echo ""
echo "You can now run:"
echo "  pnpm dev              # Run everything with hot reloading"
echo "  pnpm dev:vscode-full  # Run VSCode extension with all dependencies"
echo "  pnpm dev:playground   # Run just the playground"
echo ""
echo "For VSCode extension debugging:"
echo "  1. Run 'pnpm dev:vscode-full'"
echo "  2. Press F5 in VSCode to launch the extension host"