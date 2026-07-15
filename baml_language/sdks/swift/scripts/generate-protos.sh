#!/usr/bin/env bash
# Regenerate the Swift protobuf clients for the bridge wire format.
#
#   sdks/swift/scripts/generate-protos.sh
#   -> sdks/swift/Sources/BamlBridge/Proto/*.pb.swift
#
# Source of truth: crates/bridge_ctypes/types/baml_bridge/cffi/v1/
# (see crates/bridge_ctypes/README.md — each SDK owns its client
# generation). Generated sources are checked in so package consumers
# never need protoc; CI's proto-sync job should re-run this and fail
# on a dirty tree.
#
# Requires: protoc + protoc-gen-swift (`brew install protobuf swift-protobuf`).
set -euo pipefail

cd "$(dirname "$0")/.."  # sdks/swift
WORKSPACE_ROOT="$(cd ../.. && pwd)"
PROTO_ROOT="$WORKSPACE_ROOT/crates/bridge_ctypes/types"
OUT_DIR="Sources/BamlBridge/Proto"

for tool in protoc protoc-gen-swift; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: $tool not found — install with: brew install protobuf swift-protobuf" >&2
        exit 1
    }
done

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

protoc \
    --proto_path="$PROTO_ROOT" \
    --swift_out="$OUT_DIR" \
    --swift_opt=Visibility=Internal \
    --swift_opt=FileNaming=DropPath \
    "$PROTO_ROOT"/baml_bridge/cffi/v1/baml_handle.proto \
    "$PROTO_ROOT"/baml_bridge/cffi/v1/baml_type.proto \
    "$PROTO_ROOT"/baml_bridge/cffi/v1/baml_inbound.proto \
    "$PROTO_ROOT"/baml_bridge/cffi/v1/baml_outbound.proto

echo "wrote $(ls "$OUT_DIR" | wc -l | tr -d ' ') files to $OUT_DIR"
