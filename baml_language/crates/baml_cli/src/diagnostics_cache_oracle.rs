//! Oracle for the check-row cache: the diagnostics a warm compile *serves*
//! must render byte-identically to an honest full check of the same sources.
//!
//! A row is served only for a project byte-identical to the compile that
//! wrote it (the manifest's program key equals this compile's), so the oracle
//! has two arms: an identical reopen serves every row and must match honest;
//! any edit serves nothing and checks honestly. Plus the degradation and
//! verify cases: a corrupt row is re-checked, a faithful cache passes the
//! `DEV_BAML_BUILD_CACHE_VERIFY=always` oracle, a stale one bails.
//!
//! These round-trip through the on-disk cache on every supported platform.

use std::path::PathBuf;

use baml_db::{ProjectDatabase, SourceRoot};

use crate::{
    bytecode_cache::CacheContext,
    cache_test_support::{cache_disabled, compile_and_store_v1, open, resolved, unique_root},
    check_command::render_project_diagnostics,
    project_load,
};

/// A unique on-disk root for a diagnostics-oracle scenario.
fn oracle_root() -> PathBuf {
    unique_root("baml-diag-oracle")
}

struct OracleResult {
    served: String,
    honest: String,
    /// Whether the warm compile served the check rows at all.
    rows_served: bool,
}

#[derive(Clone, Copy)]
enum ServePath {
    RunTest,
    Check,
}

/// Compile+store `initial`, reopen as `edited`, and return the rendered served
/// vs honest (fresh full) diagnostics. `None` when the on-disk cache is
/// disabled.
fn run_scenario_with(
    initial: &[(&str, &str)],
    edited: &[(&str, &str)],
    serve_path: ServePath,
) -> Option<OracleResult> {
    if cache_disabled() {
        return None;
    }
    let root = oracle_root();
    let _ = compile_and_store_v1(&root, initial);

    // Served path: the warm preamble (seeds + rows) then the gate.
    let r2 = resolved(&root, edited);
    let (mut db2, pkg2) = project_load::build_db_from_sources(&r2, |_| {});
    let ctx2 = open(&r2).expect("cache reopens");
    let served = ctx2.prepare_warm_db(&mut db2, pkg2).served;
    let diagnostics = match serve_path {
        ServePath::RunTest => {
            ctx2.collect_diagnostics_incremental(&db2, pkg2, served.as_ref())
                .merged
        }
        ServePath::Check => ctx2.collect_diagnostics_for_check(&db2, pkg2, served.as_ref()),
    };
    let served_render = render_project_diagnostics(&db2, &diagnostics);

    // Honest path: an independent fresh database, no cache, no seed.
    let (db_honest, _) = project_load::build_db_from_sources(&r2, |_| {});
    let honest = baml_db::collect_compiler2_diagnostics(&db_honest);
    let honest_render = render_project_diagnostics(&db_honest, &honest);

    let _ = std::fs::remove_dir_all(&root);
    Some(OracleResult {
        served: served_render,
        honest: honest_render,
        rows_served: served.is_some(),
    })
}

// Shared fixtures: a Point type, a consumer, and an unrelated file.
const POINT: &str = "class Point {\n  x int\n  y int\n}\n";
const CONSUMER: &str = "function diff(p: Point) -> int {\n  p.x - p.y\n}\n";
const UNRELATED: &str = "function unrelated() -> int {\n  42\n}\n";
// A file carrying an unreachable-code warning (E0146): the statement after an
// unconditional `throw` is dead. Used wherever a scenario needs a warning that
// must survive being served from cache.
const WARN: &str = "function warns() -> int {\n  throw \"boom\"\n  0\n}\n";

#[test]
fn oracle_identical_project_serves_every_row_identically() {
    let files = [
        ("a.baml", POINT),
        ("b.baml", CONSUMER),
        ("w.baml", WARN),
        ("z.baml", UNRELATED),
    ];
    let Some(r) = run_scenario_with(&files, &files, ServePath::RunTest) else {
        return;
    };
    assert!(r.rows_served, "an identical project serves its check rows");
    assert_eq!(
        r.served, r.honest,
        "served diagnostics must render identically to the honest full check"
    );
    assert!(
        r.served.contains("E0146"),
        "the served warning must be present:\n{}",
        r.served
    );
}

#[test]
fn oracle_edit_serves_nothing_and_checks_honestly() {
    // A body-only edit to one file: nothing is served (a file's diagnostics
    // depend on every declaration it resolves, and only an identical program
    // proves a row current), the honest check runs, and the edited file's new
    // error gates.
    let m1 = "function main() -> int {\n  0\n}\n";
    let m2 = "function main() -> int {\n  \"not an int\"\n}\n";
    let initial = [("a.baml", POINT), ("b.baml", CONSUMER), ("m.baml", m1)];
    let edited = [("a.baml", POINT), ("b.baml", CONSUMER), ("m.baml", m2)];
    let Some(r) = run_scenario_with(&initial, &edited, ServePath::RunTest) else {
        return;
    };
    assert!(!r.rows_served, "an edited project serves no row");
    assert_eq!(r.served, r.honest);
    assert!(
        r.served.contains("E0001"),
        "the new type error must gate:\n{}",
        r.served
    );
}

#[test]
fn oracle_removed_file_serves_nothing() {
    // Deleting the file that defines `Point` makes the consumer's `Point`
    // reference unresolved — the project changed, so the honest check runs
    // and reproduces the error set.
    let initial = [
        ("a.baml", POINT),
        ("b.baml", CONSUMER),
        ("z.baml", UNRELATED),
    ];
    let edited = [("b.baml", CONSUMER), ("z.baml", UNRELATED)];
    let Some(r) = run_scenario_with(&initial, &edited, ServePath::RunTest) else {
        return;
    };
    assert!(!r.rows_served);
    assert_eq!(r.served, r.honest);
}

#[test]
fn check_cold_and_warm_warning_output_is_byte_identical() {
    let files = [("w.baml", WARN), ("z.baml", UNRELATED)];
    let Some(r) = run_scenario_with(&files, &files, ServePath::Check) else {
        return;
    };
    assert!(r.rows_served);
    assert_eq!(r.served, r.honest);
    assert!(
        r.served.contains("E0146"),
        "the served warning must still be rendered:\n{}",
        r.served
    );
}

#[test]
fn check_corrupt_row_degrades_to_honest_file_check() {
    if cache_disabled() {
        return;
    }
    with_stored_manifest(
        &[("w.baml", WARN), ("z.baml", UNRELATED)],
        |ctx, db, package| {
            ctx.corrupt_manifest_diagnostics_for_test("w.baml");
            let served = ctx.prepare_warm_db(db, package).served;
            assert!(served.is_some(), "the rows are still served");
            let diagnostics = ctx.collect_diagnostics_for_check(db, package, served.as_ref());
            let honest = baml_db::collect_compiler2_diagnostics(db);
            assert_eq!(
                render_project_diagnostics(db, &diagnostics),
                render_project_diagnostics(db, &honest),
                "an undecodable row must be recomputed honestly"
            );
        },
    );
}

// ── Verify oracle: passes on a faithful cache, bails on a stale one ──────────

/// Store a manifest for `files` (a warning-bearing file), then hand the
/// reopened context + a fresh database to `check`. Uses the env-independent
/// core so no `DEV_BAML_BUILD_CACHE_VERIFY=always` mutation is needed (parallel-test safe).
fn with_stored_manifest(
    files: &[(&str, &str)],
    check: impl FnOnce(&CacheContext, &mut ProjectDatabase, SourceRoot),
) {
    let root = oracle_root();
    let _ = compile_and_store_v1(&root, files);

    let r1 = resolved(&root, files);
    let (mut db2, pkg2) = project_load::build_db_from_sources(&r1, |_| {});
    let ctx2 = open(&r1).expect("cache reopens");
    check(&ctx2, &mut db2, pkg2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn verify_diagnostics_passes_for_faithful_cache() {
    if cache_disabled() {
        return;
    }
    with_stored_manifest(
        &[("w.baml", WARN), ("z.baml", UNRELATED)],
        |ctx, db, package| {
            assert!(
                ctx.check_cached_diagnostics_against_fresh(db, package)
                    .is_ok(),
                "the oracle must not bail on a faithfully-cached row"
            );
        },
    );
}

#[test]
fn verify_diagnostics_bails_on_a_stale_cache() {
    if cache_disabled() {
        return;
    }
    // The stored file's SOURCE is a warning and the project is unchanged, so
    // a warm run would serve the row. If the row is empty (a stale substitute
    // that dropped the warning) while a fresh check_file still produces it,
    // the oracle must bail.
    with_stored_manifest(
        &[("w.baml", WARN), ("z.baml", UNRELATED)],
        |ctx, db, package| {
            ctx.poison_manifest_diagnostics_for_test("w.baml");
            assert!(
                ctx.check_cached_diagnostics_against_fresh(db, package)
                    .is_err(),
                "the oracle must bail when the cached diagnostics drop a warning the fresh check has"
            );
        },
    );
}
