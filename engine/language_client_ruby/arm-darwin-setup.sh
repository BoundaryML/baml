#!/bin/bash
set -ex

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
export CC_aarch64_apple_darwin=clang
export AR_aarch64_apple_darwin=llvm-ar
export RANLIB_aarch64_apple_darwin=llvm-ranlib

# Ensure the environment is properly configured
export RUST_BACKTRACE=1 