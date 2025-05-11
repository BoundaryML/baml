#!/bin/bash

# This script builds the baml-cli binary in debug mode and copies it to the user's .baml directory
cd ../../../../engine && cargo build --bin baml-cli

# Get version information for the filename
VERSION=$(cargo pkgid baml-cli | cut -d'@' -f2)

# Convert architecture to match GitHub release format
if [ "$(uname -m)" = "arm64" ]; then
  ARCH="aarch64"
elif [ "$(uname -m)" = "x86_64" ]; then
  ARCH="x86_64"
else
  ARCH=$(uname -m)
fi

PLATFORM="apple-darwin"

# Create the versioned filename
VERSIONED_NAME="baml-cli-${VERSION}-${ARCH}-${PLATFORM}"

# Ensure the .baml directory exists in the home directory
mkdir -p ~/.baml

# Copy the binary with the versioned name to the .baml directory
cp target/debug/baml-cli ~/.baml/"$VERSIONED_NAME"
