//! PR-5 grep gate: a `PackageInterface` row is read by IDENTITY through
//! `baml_compiler2_hir_ty` — the extern locs and the `layout` queries answer
//! both lanes — never re-resolved by name from a raw interface anywhere
//! else. The allowlist is exactly the places that render or produce rows on
//! purpose, and every entry must still be earning its place.

use std::path::{Path, PathBuf};

/// The names that read a raw interface.
const GATED: [&str; 4] = [
    "mounted_interface(",
    "package_interface(",
    "mounted_type_row(",
    "ExportedType::",
];

/// Paths under `crates/` allowed to name a gated symbol, and why.
const ALLOWED: [(&str, &str); 6] = [
    (
        "baml_compiler2_hir_ty/src/",
        "the owner: rows become extern locs and layout queries here",
    ),
    ("baml_ide/src/describe.rs", "renders rows on purpose"),
    ("baml_ide/src/info.rs", "renders rows on purpose"),
    ("baml_ide/src/symbols.rs", "renders rows on purpose"),
    (
        "bex_project/src/runtime_compile.rs",
        "the runtime exporter: builds the projected interface of a mount",
    ),
    (
        "baml_db/src/check.rs",
        "coherence replay over mounted rows, recorded for the coherence epic",
    ),
];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the test crate lives under crates/")
        .to_path_buf()
}

/// A production source file's text: everything before its first
/// `#[cfg(test)]` — a test module is not a production path — for a file
/// under some crate's `src/`, excluding the test crate itself.
fn production_sources() -> Vec<(String, String)> {
    let crates = crates_dir();
    let mut sources = Vec::new();
    for entry in walkdir::WalkDir::new(&crates)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let rel = path
            .strip_prefix(&crates)
            .expect("walked under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        let Some((crate_name, rest)) = rel.split_once('/') else {
            continue;
        };
        if crate_name == "baml_tests" || !rest.starts_with("src/") {
            continue;
        }
        let text = std::fs::read_to_string(path).expect("source file reads");
        let production = match text.find("#[cfg(test)]") {
            Some(at) => text[..at].to_string(),
            None => text,
        };
        sources.push((rel, production));
    }
    assert!(!sources.is_empty(), "the walk found production sources");
    sources
}

#[test]
fn interface_rows_are_read_by_identity_outside_hir_ty() {
    let mut offenders = Vec::new();
    for (path, source) in production_sources() {
        if ALLOWED.iter().any(|(prefix, _)| path.starts_with(prefix)) {
            continue;
        }
        for needle in GATED {
            if source.contains(needle) {
                offenders.push(format!("{path}: `{needle}`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "interface rows re-resolved by name outside the allowlist:\n  {}",
        offenders.join("\n  ")
    );
}

/// Every allowlist entry still names a gated symbol; one that no longer does
/// is a stale exemption to delete.
#[test]
fn every_allowlist_entry_is_still_earning_its_place() {
    let sources = production_sources();
    let stale: Vec<&str> = ALLOWED
        .iter()
        .filter(|(prefix, _)| {
            !sources.iter().any(|(path, source)| {
                path.starts_with(prefix) && GATED.iter().any(|needle| source.contains(needle))
            })
        })
        .map(|(prefix, _)| *prefix)
        .collect();
    assert!(stale.is_empty(), "stale allowlist entries: {stale:?}");
}
