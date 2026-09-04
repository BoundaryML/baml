#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROTO_DIR="${SCRIPT_DIR}/../../crates/bridge_ctypes/types"
OUT_DIR="${SCRIPT_DIR}/baml_go/internal"

mkdir -p "${OUT_DIR}"

# Resolve protoc-gen-go
if [ -n "${PROTOC_GEN_GO_PATH:-}" ]; then
    protoc_gen_go_dir="$(dirname "${PROTOC_GEN_GO_PATH}")"
    export PATH="${protoc_gen_go_dir}:${PATH}"
fi

# Verify protoc-gen-go is available
if ! command -v protoc-gen-go &>/dev/null; then
    echo "ERROR: protoc-gen-go not found. Install with: go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.34.1" >&2
    exit 1
fi

protoc \
    --proto_path="${PROTO_DIR}" \
    --go_out="${OUT_DIR}" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_type.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_handle.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_inbound.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_outbound.proto"

echo "Generated Go proto files in ${OUT_DIR}/cffi/"
