#!/bin/bash
# https://github.com/briansmith/ring/issues/1824#issuecomment-2059955073 -- justification
# Apple Clang doesn't have comprehensive WASM support, so we need to install LLVM clang and relink Rust to use it via reinstall. This is necessary for a dependency Vertex relies on.
# Install LLVM
brew install llvm

# Add LLVM to PATH
echo 'export PATH="/opt/homebrew/opt/llvm/bin:$PATH"' >> ~/.zshrc

# Reload shell configuration
source ~/.zshrc

# Check LLVM version
llvm-config --version

# Uninstall Rust
rustup self uninstall

# Uninstall Rust via Homebrew (if necessary)
brew uninstall rust

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown

# Add x86_64 macOS target
rustup target add x86_64-apple-darwin

echo "Setup complete. Please restart your terminal for changes to take effect."