#!/bin/sh
set -eu
version=0.18.1-nightly.20260908.a
case "$1" in
  arm64) arch=aarch64 ;;
  amd64) arch=x86_64 ;;
  *) echo "Unsupported architecture: $1" >&2; exit 1 ;;
esac
case "$2" in gnu|musl) libc=$2 ;; *) exit 1 ;; esac
target="$arch-unknown-linux-$libc"
archive="baml-language-$version-$target.tar.gz"
base="https://github.com/BoundaryML/baml/releases/download/baml-language-$version"
mkdir -p /opt/baml
curl -fsSL --connect-timeout 15 --max-time 300 --retry 3 "$base/$archive" -o /tmp/baml.tar.gz
curl -fsSL --connect-timeout 15 --max-time 60 --retry 3 "$base/$archive.sha256" -o /tmp/baml.sha256
expected=$(cut -d ' ' -f 1 /tmp/baml.sha256)
echo "$expected  /tmp/baml.tar.gz" | sha256sum -c -
tar -xzf /tmp/baml.tar.gz -C /opt/baml
/opt/baml/bin/baml-cli --version
