#!/bin/bash
set -ex

# Install osxcross if not already installed
if [ ! -d "/tmp/osxcross" ]; then
    cd /tmp
    sudo rm -rf osxcross  # Clean up any failed previous attempts
    git clone https://github.com/tpoechtrager/osxcross
    cd osxcross
    mkdir -p tarballs
    wget -nc https://github.com/phracker/MacOSX-SDKs/releases/download/11.3/MacOSX11.3.sdk.tar.xz -O tarballs/MacOSX11.3.sdk.tar.xz
    UNATTENDED=1 ./build.sh
fi

# Add osxcross to PATH
export PATH="/tmp/osxcross/target/bin:$PATH"

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
export OPENSSL_NO_VENDOR=0  # Use vendored OpenSSL since we have proper tools now

# Cross compilation settings using osxcross
export AR=aarch64-apple-darwin20.4-ar
export RANLIB=aarch64-apple-darwin20.4-ranlib
export CC=aarch64-apple-darwin20.4-clang
export CXX=aarch64-apple-darwin20.4-clang++

# Set the specific tools for the target
export CC_aarch64_apple_darwin=$CC
export CXX_aarch64_apple_darwin=$CXX
export AR_aarch64_apple_darwin=$AR
export RANLIB_aarch64_apple_darwin=$RANLIB

# Ensure the environment is properly configured
export RUST_BACKTRACE=1 