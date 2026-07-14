#!/usr/bin/env bash
# Native-build setup for the sdk_test_cpp crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script binding
# in `baml_language/.config/nextest.toml` whenever the run selects any
# sdk_test_cpp test. For plain `cargo test` (no nextest) run this manually.
#
# One responsibility: build the dev-profile bridge_cffi cdylib that every
# fixture's test.sh links against (target/debug/libbridge_cffi.*). The dev
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

# Per-run breadcrumb for the in-test guard; see setup_guard in
# harness_runner and SETUP_ENV_VAR in harness_setup/src/cpp.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_CPP_SETUP=1" >> "$NEXTEST_ENV"
fi
