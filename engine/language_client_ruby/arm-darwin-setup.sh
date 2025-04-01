#!/bin/bash
set -ex

# Create a directory for our cross-compilation tools
mkdir -p /tmp/cross-tools
cd /tmp/cross-tools

# Create symlinks with the expected names
ln -sf $(which llvm-ar) aarch64-apple-darwin-ar
ln -sf $(which llvm-ranlib) aarch64-apple-darwin-ranlib
ln -sf $(which clang) aarch64-apple-darwin-cc
ln -sf $(which clang++) aarch64-apple-darwin-c++

# Add our tools directory to the PATH
export PATH="/tmp/cross-tools:$PATH"

# Ensure SDKROOT is set for macOS cross-compilation
export SDKROOT=$(xcrun --sdk macosx --show-sdk-path)

# Set deployment target to match extconf.rb
export MACOSX_DEPLOYMENT_TARGET=10.13

# Set architecture-specific flags for Ruby extension compilation
export ARCHFLAGS="-arch arm64"
export CFLAGS="-target arm64-apple-darwin"
export CXXFLAGS="-target arm64-apple-darwin"
export LDFLAGS="-target arm64-apple-darwin"

# Ruby-specific environment variables
export RUBY_TARGET=aarch64-apple-darwin
export CARGO_BUILD_TARGET=aarch64-apple-darwin

# rb-sys specific settings
export RB_SYS_FORCE_INSTALL_RUBY_VERSION=true
export RB_SYS_FORCE_INSTALL_RUBY=true

# OpenSSL specific settings - use vendored version
export OPENSSL_STATIC=1
export OPENSSL_NO_VENDOR=0  # Use vendored OpenSSL
export OPENSSL_LIB_DIR=""   # Clear any existing OpenSSL paths
export OPENSSL_INCLUDE_DIR=""

# Cross compilation settings for OpenSSL
export AR=aarch64-apple-darwin-ar
export RANLIB=aarch64-apple-darwin-ranlib
export CC=aarch64-apple-darwin-cc
export CXX=aarch64-apple-darwin-c++

# Set the specific tools for the target
export CC_aarch64_apple_darwin=$CC
export CXX_aarch64_apple_darwin=$CXX
export AR_aarch64_apple_darwin=$AR
export RANLIB_aarch64_apple_darwin=$RANLIB

# Ensure the environment is properly configured
export RUST_BACKTRACE=1 