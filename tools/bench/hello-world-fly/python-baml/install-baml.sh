#!/bin/sh
set -eu
version=0.18.1-nightly.20260908.a
target="${1:-x86_64-unknown-linux-gnu}"
archive="baml-language-$version-$target.tar.gz"
base="https://github.com/BoundaryML/baml/releases/download/baml-language-$version"
mkdir -p /opt/baml
curl -fsSL --retry 3 "$base/$archive" -o /tmp/baml.tar.gz
curl -fsSL --retry 3 "$base/$archive.sha256" -o /tmp/baml.sha256
expected=$(cut -d ' ' -f 1 /tmp/baml.sha256)
echo "$expected  /tmp/baml.tar.gz" | sha256sum -c -
tar -xzf /tmp/baml.tar.gz -C /opt/baml
/opt/baml/bin/baml-cli --version
