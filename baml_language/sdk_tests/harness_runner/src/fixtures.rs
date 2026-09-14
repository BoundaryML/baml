//! The shared fixture corpus, as one table both sides of the harness agree on.
//!
//! `sdk_test_codegen` iterates [`SHARED`] to decide what to generate; each
//! generator crate's `test_suite!` invocation names the same fixtures to
//! declare its tests. Naming them twice is the price of moving the `#[test]`
//! set into source — [`assert_shared_manifest`] is the oracle that keeps the
//! two declarations, and the directory listing, from drifting apart.

use std::{fs, path::Path};

/// Fixtures under `sdk_tests/fixtures/` that SDK codegen deliberately skips.
///
/// `host_reflect` exercises the host-reflection extraction contract, restored
/// in the later compiler slices; it stays out of SDK codegen until then.
pub const SKIPPED: &[&str] = &["host_reflect"];

/// Every fixture the generators fan out over, sorted.
pub const SHARED: &[&str] = &[
    "docstrings_etc",
    "function_calls",
    "llm_functions",
    "type_shapes",
    "unsupported_only",
];

/// The fixtures actually on disk under `fixtures_root`: every child holding a
/// `baml_src/`, minus [`SKIPPED`]. Sorted, so a drift report reads as a diff.
pub fn discover_shared(fixtures_root: &Path) -> Vec<String> {
    let entries = fs::read_dir(fixtures_root).unwrap_or_else(|error| {
        panic!(
            "failed to read the fixture corpus at {}: {error}",
            fixtures_root.display()
        )
    });
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("baml_src").is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !SKIPPED.contains(&name.as_str()))
        .collect();
    found.sort();
    found
}

/// Fail unless a generator crate's declared fixtures, [`SHARED`], and the
/// corpus on disk all agree.
///
/// `manifest_dir` is the generator crate's `CARGO_MANIFEST_DIR` — that is,
/// `<sdk_tests>/crates/<generator>` — from which the corpus root is derived.
pub fn assert_shared_manifest(manifest_dir: &str, declared: &[&str]) {
    let crate_dir = Path::new(manifest_dir);
    let fixtures_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| unreachable!("a generator crate is not at sdk_tests/crates/<generator>"))
        .join("fixtures");

    assert_eq!(
        declared,
        SHARED,
        "\n\nthis crate's `test_suite!` fixtures have drifted from the shared corpus table.\n\
         Update the `fixture <name>;` rows in {}/src/lib.rs to match \
         `sdk_test_harness_runner::fixtures::SHARED`.\n",
        crate_dir.display()
    );

    let discovered = discover_shared(&fixtures_root);
    assert_eq!(
        discovered,
        SHARED,
        "\n\nthe fixture corpus on disk has drifted from `fixtures::SHARED`.\n\
         Update SHARED in sdk_tests/harness_runner/src/fixtures.rs, then add or \
         remove the matching `fixture <name>;` row in every \
         sdk_tests/crates/*/src/lib.rs.\n\
         Corpus: {}\n",
        fixtures_root.display()
    );
}
