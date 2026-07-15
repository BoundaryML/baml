#!/usr/bin/env bash
# Native setup for the sdk_test_swift crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_swift test on a macOS host. For plain
# `cargo test` (no nextest) run this manually after
# `cargo test --no-run` populates each fixture's generated/ package.
#
# One responsibility: build (or incrementally rebuild) the host-arch
# `bridge_swift` staticlib and assemble it into
# `sdks/swift/Binaries/BamlBridgeFFI.xcframework` — the binary target
# every fixture's path dependency on `sdks/swift` links against.
# Debug profile, so the cargo cache is shared with the rest of the
# nextest workspace build and steady-state reruns are incremental.
#
# Re-run after bridge Rust changes (the xcframework needs
# reassembling) or after adding a new fixture.
set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/swift

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

echo "==> build-xcframework.sh --host-only (bridge_swift staticlib)"
"$WORKSPACE_ROOT/sdks/swift/scripts/build-xcframework.sh" --host-only

# Per-run breadcrumb for the in-test guard. nextest reads $NEXTEST_ENV
# after this script and injects these vars into the matched tests'
# processes — so `setup_guard::ran` (see harness_runner) can prove
# this script ran *this* run. Keep the var name in sync with
# SETUP_ENV_VAR in harness_setup/src/swift.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_SWIFT_SETUP=1" >> "$NEXTEST_ENV"
fi
