//! Shared by `build.rs` (producer) and `src/stdlib_prefix.rs` (consumer) so the
//! artifact header cannot drift between them.

/// Optimization levels the artifact carries a bytecode slice for. Every level a
/// test can ask for must appear here, because `prefix` has no fallback: a
/// missing level is a panic, not a silent slow path.
pub(crate) const OPT_LEVELS: [u8; 3] = [0, 1, 2];

/// `emit_test_cases` the stdlib slice is lowered with. The stdlib declares no
/// `test` blocks, so this cannot change its bytecode — it is recorded only so a
/// future stdlib that did declare one could not silently reuse this slice.
pub(crate) const EMIT_TEST_CASES: bool = false;

/// Guards against a producer/consumer format mismatch *within* one build.
///
/// Cargo already reruns the build script whenever the compiler dependency graph
/// changes, so embedded bytes cannot outlive the build that produced them; this
/// key is belt-and-braces for a hand-copied artifact.
pub(crate) fn artifact_key() -> String {
    format!(
        "baml-tests-stdlib-prefix-v1:version={}:channel={}:opts={OPT_LEVELS:?}:emit_test_cases={EMIT_TEST_CASES}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}
