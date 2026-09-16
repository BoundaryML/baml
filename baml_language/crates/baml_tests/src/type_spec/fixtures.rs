//! Directory-driven type conformance checks. Every conforming fixture must
//! satisfy its caret annotations and clean-by-default error channel. Pending
//! fixtures must still fail, and promotion is required when they turn green.
//!
//! Only SNAPSHOT_EXAMPLES gets a full inferred-node dump. New fixtures add
//! assertion coverage without automatically adding another golden file.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::harness::{FixtureOutcome, run_fixture};

// Keep in sync with this test's threads-required override in nextest.toml.
// A private pool also bounds nested Rayon work inside compiler queries.
const FIXTURE_WORKERS: usize = 4;

struct CheckedFixture {
    name: String,
    snapshot: bool,
    outcome: FixtureOutcome,
}

fn check_conforming_fixtures(paths: &[PathBuf], workers: usize) -> Vec<CheckedFixture> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|index| format!("type-fixture-{index}"))
        .build()
        .expect("create bounded type-fixture pool");
    pool.install(|| {
        // par_iter is indexed: collect preserves the sorted input order even
        // when checks finish out of order. No Salsa database crosses workers;
        // run_fixture constructs and drops one locally for each source.
        paths
            .par_iter()
            .map(|path| {
                let name = fixture_name(path);
                let fixture = std::fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("cannot read fixture {}: {err}", path.display()));
                let snapshot = SNAPSHOT_EXAMPLES
                    .iter()
                    .any(|(example, _)| *example == name);
                CheckedFixture {
                    name,
                    snapshot,
                    outcome: run_fixture(&fixture, snapshot),
                }
            })
            .collect()
    })
}

// Golden dumps are opt-in. All fixtures still assert their caret annotations
// and clean-by-default error channel; new fixtures do not create snapshots.
const SNAPSHOT_EXAMPLES: &[(&str, &str)] = &[
    (
        "reduce_literal_seed_widens",
        "Generic inference widens a literal seed.",
    ),
    (
        "null_guard_early_return",
        "Flow narrowing survives a diverging branch.",
    ),
    (
        "lambda_param_from_method_slot",
        "Contextual lambda parameter inference.",
    ),
    (
        "throws_mutual_recursion_fixpoint",
        "Recursive effect inference reaches a fixpoint.",
    ),
    (
        "match_rigid_arm_not_exhaustive",
        "Generic match error and non-exhaustiveness dump.",
    ),
];

#[test]
fn snapshot_examples_are_valid() {
    let mut names = std::collections::BTreeSet::new();
    for (name, reason) in SNAPSHOT_EXAMPLES {
        assert!(names.insert(name), "duplicate snapshot example: {name}");
        assert!(!reason.trim().is_empty(), "missing rationale: {name}");
        assert!(
            fixtures_root().join(format!("{name}.baml")).is_file(),
            "snapshot example missing or moved to pending: {name}"
        );
    }
    let prefix = "baml_tests__type_spec__fixtures__";
    let expected: std::collections::BTreeSet<_> = SNAPSHOT_EXAMPLES
        .iter()
        .map(|(name, _)| format!("{prefix}{name}.snap"))
        .collect();
    let actual: std::collections::BTreeSet<_> =
        std::fs::read_dir(fixtures_root().parent().unwrap().join("snapshots"))
            .expect("read type-spec snapshots")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(prefix) && name.ends_with(".snap"))
            .collect();
    assert_eq!(actual, expected, "missing or orphaned type-fixture goldens");
}

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
    let paths = fixture_paths(&fixtures_root(), true);
    // Keep Insta and failure reporting on the test thread, in fixture order.
    for CheckedFixture {
        name,
        snapshot,
        outcome,
    } in check_conforming_fixtures(&paths, FIXTURE_WORKERS)
    {
        if let Err(report) = &outcome.hir_ty {
            failures.push(format!("{name}:\n  {report}"));
        }
        if snapshot {
            insta::assert_snapshot!(name, outcome.dump);
        }
    }
    assert!(
        failures.is_empty(),
        "fixture check failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn bounded_pool_preserves_order_and_results() {
    // Deliberately use nonalphabetical order to check indexed collection,
    // including both a golden and an ordinary assertion-only fixture.
    let paths = [
        "throws_mutual_recursion_fixpoint",
        "call_result_type",
        "null_guard_early_return",
    ]
    .map(|name| fixtures_root().join(format!("{name}.baml")));
    let serial = check_conforming_fixtures(&paths, 1);
    let parallel = check_conforming_fixtures(&paths, FIXTURE_WORKERS);
    for ((serial, parallel), path) in serial.iter().zip(&parallel).zip(&paths) {
        assert_eq!(serial.name, fixture_name(path));
        assert_eq!(parallel.name, serial.name);
        assert_eq!(parallel.snapshot, serial.snapshot);
        assert_eq!(parallel.outcome.hir_ty, serial.outcome.hir_ty);
        assert_eq!(parallel.outcome.dump, serial.outcome.dump);
        assert!(parallel.outcome.hir_ty.is_ok());
    }
    assert_eq!(parallel.len(), paths.len());
}

#[test]
fn dump_selection_preserves_annotation_checks() {
    let source = include_str!("fixtures/reduce_literal_seed_widens.baml");
    let with_dump = run_fixture(source, true);
    let without_dump = run_fixture(source, false);
    assert!(with_dump.hir_ty.is_ok());
    assert_eq!(with_dump.hir_ty, without_dump.hir_ty);
    assert!(!with_dump.dump.is_empty());
    assert!(without_dump.dump.is_empty());

    // Disabling a dump must not turn a wrong inferred-type expectation green.
    let wrong_expectation = source.replacen("// ^^^^^ int", "// ^^^^^ string", 1);
    assert_ne!(source, wrong_expectation);
    let with_dump = run_fixture(&wrong_expectation, true);
    let without_dump = run_fixture(&wrong_expectation, false);
    assert!(with_dump.hir_ty.is_err());
    assert_eq!(with_dump.hir_ty, without_dump.hir_ty);
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
        let outcome = run_fixture(&fixture, false);
        if outcome.hir_ty.is_ok() {
            failures.push(format!(
                "{name}: pending fixture now PASSES under hir_ty; its slice has landed. \
                 Promote it to fixtures/ and drop the `// pending:` directive."
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "pending fixture failures:\n{}",
        failures.join("\n")
    );
}
