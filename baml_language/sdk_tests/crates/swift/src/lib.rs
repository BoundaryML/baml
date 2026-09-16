//! Swift sdk-test crate. One `swift test` per fixture, declared below and
//! expanded by `sdk_test_harness_runner::swift::test_suite!`.
//!
//! Fixtures run for real on macOS only — there is no Swift toolchain on the
//! other CI hosts. A `later` row marks a fixture whose suite is known broken
//! on macOS too; it carries its own reason, since the causes differ.
//!
//! Note that CI does not currently run this crate at all: the sdk-test matrix
//! in `.github/workflows/cargo-tests.reusable.yaml` filters out every macOS
//! entry, and Swift has no other. Both `later` rows below were found by
//! running the suite locally.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::swift::test_suite! {
    fixture docstrings_etc;
    fixture function_calls later
        "TestUnhandledSpawnErrors/test_unhandled_spawn_error_uses_host_default hangs: \
         it re-execs itself via `xctest` and calls waitUntilExit() before draining \
         the child's stderr pipe, and the child never exits";
    fixture llm_functions later
        "sdkgen_swift does not emit the stream_e2e_* streaming projections \
         TestStreamingE2E.swift calls — see _BamlSkipped.swift in the generated tree";
    fixture type_shapes;
    // No Swift overlay (placeholder test only): everything in it is within
    // Swift's type algebra, so it runs as a codegen-compiles integrity check.
    fixture unsupported_only;
}
