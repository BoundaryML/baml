#!/usr/bin/env bash
# Native-build setup for the sdk_test_cpp crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script binding
# in `baml_language/.config/nextest.toml` whenever the run selects any
# sdk_test_cpp test. For plain `cargo test` (no nextest) run this manually.
#
# One responsibility: build the dev-profile bridge_cffi cdylib that every
# fixture's test.sh dlopens at run time (target/debug/libbridge_cffi.*). The dev
# profile has panic=unwind by default, matching the release-bridge-cffi
# shipping profile's unwind requirement. Features mirror the workspace test
# convention (ring-crypto instead of the default aws-crypto).
#
# This placement (out of build.rs) mirrors the python/typescript targets so
# `cargo check`/`cargo doc` succeed without a C++ toolchain and the heavy
# work fires only when a cpp sdk-test is actually selected.
set -euo pipefail

cd "$(dirname "$0")" # baml_language/sdk_tests/crates/cpp

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

echo "==> cargo build -p bridge_cffi (dev cdylib for cpp sdk tests)"
(cd "$WORKSPACE_ROOT" && cargo build -p bridge_cffi --no-default-features --features ring-crypto,bundle-http)

# Pre-clone the pinned protobuf + abseil sources once. Every build tree
# consumes them via FETCHCONTENT_SOURCE_DIR_* overrides (see cpp_test.sh /
# tests/run.sh), which skips FetchContent population entirely: concurrent
# cmake configures never race and fixtures need no network. Tags must match
# bridge_cpp/cmake/fetch_protobuf.cmake and protobuf's own
# cmake/dependencies.cmake abseil pin.
PROTOBUF_SRC="$WORKSPACE_ROOT/target/cpp-protobuf-src"
ABSL_SRC="$WORKSPACE_ROOT/target/cpp-absl-src"
if [[ ! -d "$PROTOBUF_SRC" ]]; then
    echo "==> clone pinned protobuf v31.1"
    git clone --quiet --depth 1 --branch v31.1 \
        https://github.com/protocolbuffers/protobuf.git "$PROTOBUF_SRC"
fi
if [[ ! -d "$ABSL_SRC" ]]; then
    echo "==> clone pinned abseil 20250127.0"
    git clone --quiet --depth 1 --branch 20250127.0 \
        https://github.com/abseil/abseil-cpp.git "$ABSL_SRC"
fi

# Per-run breadcrumb for the in-test guard; see setup_guard in
# harness_runner and SETUP_ENV_VAR in harness_setup/src/cpp.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_CPP_SETUP=1" >> "$NEXTEST_ENV"
fi
