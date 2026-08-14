#!/usr/bin/env bash
# Per-fixture cargo pre-warm for the sdk_test_rust crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_rust test. For plain `cargo test` (no nextest)
# run this manually after `cargo test --no-run` populates each
# fixture's `generated/` crate.
#
# Unlike the python/node targets there is no package manager or native
# addon to install — the only toolchain is cargo itself. What this
# script buys:
#
#   1. A serial `cargo test --no-run` per fixture into the shared
#      CARGO_TARGET_DIR pre-builds the bridge_rust → BEX runtime stack
#      once, so the test-time `cargo clippy` / `cargo test` invocations
#      (which nextest fans out in parallel, all flock-queueing on the
#      same target dir) hit a warm cache instead of racing a cold build.
#
#   2. The $NEXTEST_ENV breadcrumb the emitted `setup_guard::ran` test
#      asserts on, keeping this target's CI shape identical to the
#      other sdk-test targets.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/rust

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

# The generated SDKs load the engine as a shared library at run time
# (baml_bridge is dylib-only). Build the cdylib into the MAIN workspace
# target dir — before the fixture CARGO_TARGET_DIR export below — which
# is where the emitted tests look for it (next to their own binary, so
# ambient CARGO_TARGET_DIR/profile agree by construction).
echo "==> cargo build -p bridge_cffi (engine cdylib)"
(cd "$WORKSPACE_ROOT" && cargo build -p bridge_cffi)

# Shared cargo build dir under target/, matching the CARGO_TARGET_DIR
# the emitted tests thread through (run_test_cmd / CACHE_SUBDIR in
# harness_setup/src/rust.rs).
export CARGO_TARGET_DIR="$WORKSPACE_ROOT/target/sdk-rust-target"
mkdir -p "$CARGO_TARGET_DIR"

for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    # Never run cargo without the generated manifest: cargo discovers
    # manifests upward, so a missing Cargo.toml (codegen failure) would
    # silently turn this into a workspace-wide build. The failure itself
    # surfaces via the build_diagnostics test.
    if [[ ! -f "$fixture_dir/Cargo.toml" ]]; then
        echo "==> skipping $fixture_dir (no Cargo.toml — codegen failed?)"
        continue
    fi
    echo "==> cargo test --no-run in $fixture_dir"
    (cd "$fixture_dir" && cargo test --no-run --manifest-path Cargo.toml)
done

# Per-run breadcrumb for the in-test guard. nextest reads $NEXTEST_ENV
# after this script and injects these vars into the matched tests'
# processes — so `setup_guard::ran` (see harness_runner) can prove
# this script ran *this* run. Plain `cargo test` has no $NEXTEST_ENV,
# so the var stays unset and the guard fails with a helpful message.
# Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/rust.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_RUST_SETUP=1" >> "$NEXTEST_ENV"
fi
