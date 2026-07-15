#!/usr/bin/env bash
# Per-fixture Gradle setup for the sdk_test_java crate — Unix.
# Windows uses the parallel `setup.ps1`; keep the two in sync.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_java test. Run the suite with
# `cargo nextest run`, not `cargo test`: plain `cargo test` skips this
# script and can't pass `setup_guard::ran` (see ../../README.md).
#
# Stub era: `sdkgen_java` hasn't landed, every generated test is
# `#[ignore]`d, and this script only stages the shared Gradle home and
# writes the setup_guard breadcrumb. When the Java bridge lands, this
# script additionally needs to (mirroring typescript_node/setup.sh):
#   1. build the native bridge library the fixtures link against
#      (`cargo build -p bridge_cffi` or the bridge_java crate's build),
#   2. warm the per-fixture Gradle dependency resolution so the tests'
#      `gradle` invocations only do read-only work.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/java

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

# Shared Gradle home under target/ so dependency, wrapper, and
# provisioned-JDK caches land in one place rather than in ~/.gradle.
# Keep in sync with CACHE_SUBDIR / CACHE_ENV_VAR in
# harness_setup/src/java.rs.
export GRADLE_USER_HOME="$WORKSPACE_ROOT/target/gradle-home"
mkdir -p "$GRADLE_USER_HOME"

if ! command -v gradle >/dev/null 2>&1 && ! mise which gradle >/dev/null 2>&1; then
    echo "warning: gradle not found on PATH (or via mise); sdk_test_java tests will fail once un-ignored" >&2
fi

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ../../DEVELOPMENT.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/java.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_JAVA_SETUP=1" >> "$NEXTEST_ENV"
fi
