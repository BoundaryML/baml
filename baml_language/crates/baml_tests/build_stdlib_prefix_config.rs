//! Shared by `build.rs` (producer) and `src/stdlib_prefix.rs` (consumer) so the
//! artifact header cannot drift between them.

/// Optimization levels the artifact carries a bytecode slice for. Every level a
/// test can ask for must appear here, because `prefix` has no fallback: a
/// missing level is a panic, not a silent slow path.
pub(crate) const OPT_LEVELS: [u8; 3] = [0, 1, 2];

/// Guards against a producer/consumer format mismatch *within* one build.
///
/// Cargo already reruns the build script whenever the compiler dependency graph
/// changes, so embedded bytes cannot outlive the build that produced them; this
/// key is belt-and-braces for a hand-copied artifact.
pub(crate) fn artifact_key() -> String {
    format!(
        "baml-tests-stdlib-prefix-v2:version={}:channel={}:opts={OPT_LEVELS:?}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}
