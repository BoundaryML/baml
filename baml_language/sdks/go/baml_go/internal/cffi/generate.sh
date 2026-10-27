#!/usr/bin/env bash
# Regenerate the Go protobuf bindings of the wire schemas
# (`crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto`) into this
# directory. The output is checked in; CI regenerates it and diffs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROTO_DIR="${SCRIPT_DIR}/../../../../../crates/bridge_ctypes/types"

# Resolve protoc-gen-go
if [ -n "${PROTOC_GEN_GO_PATH:-}" ]; then
    protoc_gen_go_dir="$(dirname "${PROTOC_GEN_GO_PATH}")"
    export PATH="${protoc_gen_go_dir}:${PATH}"
fi

if ! command -v protoc-gen-go &>/dev/null; then
    echo "ERROR: protoc-gen-go not found. Install with: go install google.golang.org/protobuf/cmd/protoc-gen-go@latest" >&2
    exit 1
fi

# `source_relative` places the files under `baml_bridge/cffi/v1/`; this
# package keeps them flat, so generate aside and move them in.
STAGE="$(mktemp -d)"
trap 'rm -rf "${STAGE}"' EXIT
protoc \
    --proto_path="${PROTO_DIR}" \
    --go_out="${STAGE}" \
    --go_opt=paths=source_relative \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_type.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_handle.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_inbound.proto" \
    "${PROTO_DIR}/baml_bridge/cffi/v1/baml_outbound.proto"
mv "${STAGE}"/baml_bridge/cffi/v1/*.pb.go "${SCRIPT_DIR}/"

echo "Generated Go proto files in ${SCRIPT_DIR}/"
