#!/bin/sh
set -eu

toolchain_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' baml.toml)"
if [ -z "$toolchain_version" ]; then
  echo "error: baml.toml must pin [toolchain].version" >&2
  exit 1
fi

machine="$(uname -m)"
system="$(uname -s)"
case "$system:$machine" in
  Darwin:arm64) target="aarch64-apple-darwin" ;;
  Darwin:x86_64) target="x86_64-apple-darwin" ;;
  Linux:aarch64|Linux:arm64) target="aarch64-unknown-linux-musl" ;;
  Linux:x86_64) target="x86_64-unknown-linux-musl" ;;
  *) echo "error: unsupported platform $system $machine" >&2; exit 2 ;;
esac

tmp="$(mktemp -d "${TMPDIR:-/tmp}/baml-generate.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
manifest="$tmp/manifest.json"
archive="$tmp/wrapper.tar.gz"
curl -fsSL "https://pkg.boundaryml.com/manifest/v1/wrapper.json" -o "$manifest"

url="$(node -e 'const artifact = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).artifacts[process.argv[2]]; if (artifact) process.stdout.write(artifact.url)' "$manifest" "$target")"
expected_sha256="$(node -e 'const artifact = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).artifacts[process.argv[2]]; if (artifact) process.stdout.write(artifact.sha256)' "$manifest" "$target")"
if [ -z "$url" ] || [ -z "$expected_sha256" ]; then
  echo "error: BAML wrapper manifest has no artifact for $target" >&2
  exit 3
fi

curl -fsSL "$url" -o "$archive"
if command -v sha256sum >/dev/null 2>&1; then
  actual_sha256="$(sha256sum "$archive" | awk '{print $1}')"
else
  actual_sha256="$(shasum -a 256 "$archive" | awk '{print $1}')"
fi
if [ "$actual_sha256" != "$expected_sha256" ]; then
  echo "error: BAML wrapper checksum mismatch" >&2
  exit 4
fi

tar -xzf "$archive" -C "$tmp"
export BAML_HOME="$tmp/home"
"$tmp/bin/baml" toolchain use "$toolchain_version"
"$tmp/bin/baml" generate
