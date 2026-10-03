//! Shared by `build.rs` (producer) and `src/lib.rs` (consumer) so the
//! artifact header cannot drift between them.

/// Optimization levels to embed as independently decoded artifacts. Every
/// level a test can ask for must appear here; missing artifacts fail the build.
pub(crate) const OPT_LEVELS: [u8; 3] = [0, 1, 2];

/// Guards against a producer/consumer format mismatch *within* one build.
///
/// Cargo already reruns the build script whenever the compiler dependency graph
/// changes, so embedded bytes cannot outlive the build that produced them; this
/// key is belt-and-braces for a hand-copied artifact.
pub(crate) fn artifact_key(opt: u8) -> String {
    assert!(
        OPT_LEVELS.contains(&opt),
        "unsupported optimization level {opt}"
    );
    format!(
        "baml-tests-stdlib-prefix-v3:version={}:channel={}:opt={opt}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}
