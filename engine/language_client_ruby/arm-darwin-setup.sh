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

# OpenSSL specific settings
export OPENSSL_STATIC=1
export OPENSSL_NO_VENDOR=0  # Use vendored OpenSSL
export OPENSSL_LIB_DIR=""   # Let the vendored build handle this
export OPENSSL_INCLUDE_DIR="" # Let the vendored build handle this

# Disable OpenSSL features that might cause issues
export OPENSSL_NO_ASM=1
export OPENSSL_NO_SHARED=1
export OPENSSL_NO_ASYNC=1
export OPENSSL_NO_ENGINE=1
export OPENSSL_NO_DEPRECATED=1
export OPENSSL_NO_CAMELLIA=1
export OPENSSL_NO_IDEA=1
export OPENSSL_NO_SEED=1
export OPENSSL_NO_RC2=1
export OPENSSL_NO_RC4=1
export OPENSSL_NO_RC5=1
export OPENSSL_NO_MD2=1
export OPENSSL_NO_MD4=1
export OPENSSL_NO_MDC2=1
export OPENSSL_NO_WHIRLPOOL=1
export OPENSSL_NO_COMP=1
export OPENSSL_NO_ZLIB=1
export OPENSSL_NO_DYNAMIC_ENGINE=1

# Cross compilation settings
export AR=aarch64-apple-darwin-ar
export RANLIB=aarch64-apple-darwin-ranlib
export CC=aarch64-apple-darwin-cc
export CXX=aarch64-apple-darwin-c++
export LD=aarch64-apple-darwin-cc

# Set the specific tools for the target
export CC_aarch64_apple_darwin=$CC
export CXX_aarch64_apple_darwin=$CXX
export AR_aarch64_apple_darwin=$AR
export RANLIB_aarch64_apple_darwin=$RANLIB
export LD_aarch64_apple_darwin=$LD

# Additional environment variables for cross-compilation
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH=""
export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=$CC
export CARGO_TARGET_AARCH64_APPLE_DARWIN_AR=$AR
export CARGO_TARGET_AARCH64_APPLE_DARWIN_RANLIB=$RANLIB

# Ensure the environment is properly configured
export RUST_BACKTRACE=1 