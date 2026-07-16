#!/usr/bin/env bash
# Build the engine cdylib that baml_bridge's tests load at run time
# (baml_bridge is dylib-only — the engine is never linked in).
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` whenever a baml_bridge
# test is selected. For plain `cargo test -p baml_bridge` run
# `cargo build -p bridge_cffi` manually first.

set -euo pipefail

cd "$(dirname "$0")/../../.."  # baml_language workspace root

echo "==> cargo build -p bridge_cffi (engine cdylib for baml_bridge tests)"
cargo build -p bridge_cffi
