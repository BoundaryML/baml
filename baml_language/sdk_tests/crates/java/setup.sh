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
# Mirrors typescript_node/setup.sh: it builds the two artifacts every
# fixture links against, then stages the shared Gradle home and writes
# the setup_guard breadcrumb.
#   1. the native bridge library (`bridge_java` cdylib →
#      target/debug/libbridge_java.so) that the generated `Baml` anchor
#      loads via System.load, and
#   2. the `baml_bridge` runtime jar (build/libs/baml-bridge.jar) that
#      the per-fixture build.gradle.kts links with `implementation(files(...))`.
#
# NOTE: the fixtures load the native lib from the env var
# BAML_JAVA_BRIDGE_LIB, which must point at target/debug/libbridge_java.so.
# We deliberately export nothing here — env propagation happens at test
# invocation (the fixture build.gradle.kts forwards BAML_JAVA_BRIDGE_LIB
# into the Test task). Teaching harness_runner to set that var on the test
# command's environment is a follow-up.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/java

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

# Shared Gradle home under target/ so dependency, wrapper, and
# provisioned-JDK caches land in one place rather than in ~/.gradle.
# Keep in sync with CACHE_SUBDIR / CACHE_ENV_VAR in
# harness_setup/src/java.rs.
export GRADLE_USER_HOME="$WORKSPACE_ROOT/target/gradle-home"
mkdir -p "$GRADLE_USER_HOME"

if command -v gradle >/dev/null 2>&1; then
    GRADLE=(gradle)
elif mise which gradle >/dev/null 2>&1; then
    GRADLE=(mise exec -- gradle)
else
    GRADLE=()
    echo "warning: gradle not found on PATH (or via mise); sdk_test_java tests will fail once un-ignored" >&2
fi

# 1. Native bridge library the fixtures load at runtime (the JVM analog of
#    typescript_node's `.node` addon build). Toolchain pinning comes from the
#    workspace rust-toolchain.toml (1.93.0), which whichever cargo is on PATH
#    honors - the rustup shim resolves it on the mise arm, and the nix CI
#    shell ships that exact toolchain directly. An explicit `rustup run` here
#    breaks the nix arm outright (its rustup owns no toolchains) and is
#    redundant on the mise arm. Produces target/debug/libbridge_java.so.
echo "==> cargo build -p bridge_java (native bridge library)"
(cd "$WORKSPACE_ROOT" && cargo build -p bridge_java)

# 2. baml_bridge runtime jar the fixtures link against. Shares GRADLE_USER_HOME
#    (set above) so the dependency/JDK caches are warmed for the per-fixture
#    `gradle` runs. Skipped only if no gradle launcher is available (already
#    warned above); the per-fixture builds would then fail loudly anyway.
if [[ ${#GRADLE[@]} -gt 0 ]]; then
    echo "==> gradle jar (baml_bridge runtime library)"
    "${GRADLE[@]}" -p "$WORKSPACE_ROOT/sdks/java/baml_bridge" jar
fi

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ../../DEVELOPMENT.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/java.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_JAVA_SETUP=1" >> "$NEXTEST_ENV"
fi
