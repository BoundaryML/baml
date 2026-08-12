//! Directory-driven fixture runner: every `.baml` file under `fixtures/` is a
//! test, with no per-fixture Rust function, run differentially against the
//! `baml_compiler2_hir_ty` engine and TIR.
//!
//! Expectations are per engine:
//! - hir_ty is governed by directory: `fixtures/*.baml` must pass its
//!   annotation check; `fixtures/pending/*.baml` (first line
//!   `// pending: <slice> <reason>`) must FAIL it, and when a slice turns
//!   one green the runner prompts its promotion into `fixtures/`.
//! - TIR is governed by a marker: it must pass every fixture unless a
//!   `// tir: fails` line is present. The set of marked fixtures is exactly
//!   the list of places the spec corpus is ahead of the old engine.
//!
//! Every fixture also snapshots the merged infer dump (agreement lines plus
//! `hir_ty=[..] tir=[..]` difference lines), so the node-level distance
//! between the two systems is visible per fixture and shrinks in snapshot
//! review as slices land.
//!
//! These live outside the `projects/` tiers on purpose: the `compiles` tier
//! forbids diagnostics and snapshots five phases per project, while a spec
//! fixture wants exactly one cheap snapshot, and pending fixtures
//! legitimately fail.

use std::path::{Path, PathBuf};

use super::harness::run_differential;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/type_spec/fixtures")
}

fn fixture_paths(dir: &Path, allow_empty: bool) -> Vec<PathBuf> {
    // A missing directory is an empty one: `pending/` disappears with its
    // last promoted fixture (git tracks no empty dirs), and empty pending
    // is the GOAL state, not an error.
    if allow_empty && !dir.exists() {
        return Vec::new();
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("cannot read fixture dir {}: {err}", dir.display()))
        .map(|entry| entry.expect("readable dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "baml"))
        .collect();
    paths.sort();
    assert!(
        allow_empty || !paths.is_empty(),
        "no .baml fixtures found in {}",
        dir.display()
    );
    paths
}

fn fixture_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .expect("fixture file names are UTF-8")
        .to_owned()
}

#[test]
fn conforming_fixtures() {
    // Empty until the first engine slice turns a pending fixture green.
    let mut failures = Vec::new();
    for path in fixture_paths(&fixtures_root(), true) {
        let name = fixture_name(&path);
        let fixture = std::fs::read_to_string(&path).expect("readable fixture");
        let outcome = run_differential(&fixture);
        if let Err(report) = &outcome.hir_ty {
            failures.push(format!("{name}:\n  {report}"));
        }
        insta::assert_snapshot!(name, outcome.dump);
    }
    assert!(
        failures.is_empty(),
        "fixture check failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn pending_fixtures() {
    // Empty is a success state: every written pin's slice has landed.
    let mut failures = Vec::new();
    for path in fixture_paths(&fixtures_root().join("pending"), true) {
        let name = fixture_name(&path);
        let fixture = std::fs::read_to_string(&path).expect("readable fixture");
        match fixture
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("// pending: "))
        {
            Some(reason) if !reason.trim().is_empty() => {}
            _ => failures.push(format!(
                "{name}: pending fixtures must start with `// pending: <slice> <reason>`"
            )),
        }
        let outcome = run_differential(&fixture);
        if outcome.hir_ty.is_ok() {
            failures.push(format!(
                "{name}: pending fixture now PASSES under hir_ty; its slice has landed. \
                 Promote it to fixtures/ and drop the `// pending:` directive."
            ));
        }
        insta::assert_snapshot!(format!("pending__{name}"), outcome.dump);
    }
    assert!(
        failures.is_empty(),
        "pending fixture failures:\n{}",
        failures.join("\n")
    );
}
