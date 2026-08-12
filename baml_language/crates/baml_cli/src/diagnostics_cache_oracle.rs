//! Oracle for the per-file diagnostics cache: on a warm incremental compile the
//! *served* diagnostics — clean files from cache, dirty files freshly checked —
//! must render byte-identically to an honest full check of the edited sources.
//!
//! Each scenario compiles+stores `v1`, edits to `v2`, then compares the served
//! set (`collect_diagnostics_incremental` through the reuse plan) against the
//! honest set (`collect_compiler2_diagnostics` on an independent fresh
//! database). The scenario matrix mirrors `relink_oracle` — body edit, signature
//! edit, add-fn, add-class, layout reorder, throws edit, delete-file — plus two
//! cache-served cases: a warning in a clean file must survive the incremental
//! compile, and a new error in a dirty file must still gate.
//!
//! These round-trip through the on-disk cache on every supported platform.

use std::{collections::HashSet, path::PathBuf};

use baml_project::ProjectDatabase;

use crate::{
    bytecode_cache::{CacheContext, prepare_reuse_plan},
    cache_test_support::{
        cache_disabled, compile_and_store_v1, dirty_basenames, resolved, unique_root,
    },
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
    dirty: HashSet<String>,
}

#[derive(Clone, Copy)]
enum ServePath {
    RunTest,
    Check,
}

/// Compile+store `initial`, edit to `edited`, and return the rendered served
/// (incremental) vs honest (fresh full) diagnostics plus the dirty file set.
/// `None` when the on-disk cache is disabled.
fn run_scenario(initial: &[(&str, &str)], edited: &[(&str, &str)]) -> Option<OracleResult> {
    run_scenario_with(initial, edited, ServePath::RunTest)
}

fn run_check_scenario(initial: &[(&str, &str)], edited: &[(&str, &str)]) -> Option<OracleResult> {
    run_scenario_with(initial, edited, ServePath::Check)
}

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

    // v2 served path: reuse plan + seed + incremental gate.
    let r2 = resolved(&root, edited);
    let mut db2 = project_load::build_db_from_sources(&r2, |_| {});
    let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
    let pending_plan = ctx2.plan_reuse(&db2);
    let plan = prepare_reuse_plan(&mut db2, pending_plan);
    let served = match serve_path {
        ServePath::RunTest => {
            ctx2.collect_diagnostics_incremental(&db2, plan.as_ref())
                .merged
        }
        ServePath::Check => ctx2.collect_diagnostics_for_check(&db2, plan.as_ref()),
    };
    let served_render = render_project_diagnostics(&db2, &served);

    let dirty: HashSet<String> = plan
        .as_ref()
        .map(|p| dirty_basenames(&p.dirty_files, &db2))
        .unwrap_or_default();

    // v2 honest path: an independent fresh database, no cache, no seed.
    let db_honest = project_load::build_db_from_sources(&r2, |_| {});
    let honest = baml_project::collect_compiler2_diagnostics(&db_honest);
    let honest_render = render_project_diagnostics(&db_honest, &honest);

    let _ = std::fs::remove_dir_all(&root);
    Some(OracleResult {
        served: served_render,
        honest: honest_render,
        dirty,
    })
}

/// Assert served == honest for a scenario; returns the dirty set for extra
/// assertions. A skipped run (disabled cache) returns `None`.
fn assert_served_equals_honest(
    initial: &[(&str, &str)],
    edited: &[(&str, &str)],
) -> Option<HashSet<String>> {
    let r = run_scenario(initial, edited)?;
    assert_eq!(
        r.served, r.honest,
        "served (incremental) diagnostics must render identically to the honest full check"
    );
    Some(r.dirty)
}

// Shared fixtures: a Point type, a consumer, and an unrelated file.
const POINT_V1: &str = "class Point {\n  x int\n  y int\n}\n";
const POINT_V2_REORDER: &str = "class Point {\n  y int\n  x int\n}\n";
const CONSUMER: &str = "function diff(p: Point) -> int {\n  p.x - p.y\n}\n";
const UNRELATED: &str = "function unrelated() -> int {\n  42\n}\n";
// A clean file carrying an unreachable-code warning (E0146): the statement after
// an unconditional `throw` is dead. Used wherever a scenario needs a warning that
// must survive being served from cache.
const WARN: &str = "function warns() -> int {\n  throw \"boom\"\n  0\n}\n";

// ── Scenario 1: body edit (clean deps unchanged) ─────────────────────────────

#[test]
fn oracle_body_edit() {
    let m1 = "function main() -> int {\n  diff(Point { x: 1, y: 2 })\n}\n";
    let m2 = "function main() -> int {\n  diff(Point { x: 9, y: 8 }) + 1\n}\n";
    let initial = [
        ("a.baml", POINT_V1),
        ("b.baml", CONSUMER),
        ("m.baml", m1),
        ("z.baml", UNRELATED),
    ];
    let edited = [
        ("a.baml", POINT_V1),
        ("b.baml", CONSUMER),
        ("m.baml", m2),
        ("z.baml", UNRELATED),
    ];
    let Some(dirty) = assert_served_equals_honest(&initial, &edited) else {
        return;
    };
    assert!(
        !dirty.contains("z.baml"),
        "unrelated stays clean: {dirty:?}"
    );
}

// ── Scenario 2: signature edit (dirties referencing files) ───────────────────

#[test]
fn oracle_signature_edit() {
    let b1 = "function diff(p: Point) -> int {\n  p.x - p.y\n}\n";
    let b2 = "function diff(p: Point) -> string {\n  \"changed\"\n}\n";
    let m = "function main() -> int {\n  let p = Point { x: 1, y: 2 };\n  diff(p);\n  0\n}\n";
    let initial = [("a.baml", POINT_V1), ("b.baml", b1), ("m.baml", m)];
    let edited = [("a.baml", POINT_V1), ("b.baml", b2), ("m.baml", m)];
    assert_served_equals_honest(&initial, &edited);
}

// ── Scenario 3: add-fn ───────────────────────────────────────────────────────

#[test]
fn oracle_add_fn() {
    let b1 = CONSUMER;
    let b2 = "function diff(p: Point) -> int {\n  p.x - p.y\n}\n\
              function sum(p: Point) -> int {\n  p.x + p.y\n}\n";
    let initial = [("a.baml", POINT_V1), ("b.baml", b1), ("z.baml", UNRELATED)];
    let edited = [("a.baml", POINT_V1), ("b.baml", b2), ("z.baml", UNRELATED)];
    assert_served_equals_honest(&initial, &edited);
}

// ── Scenario 4: add-class ────────────────────────────────────────────────────

#[test]
fn oracle_add_class() {
    let a2 = "class Point {\n  x int\n  y int\n}\nclass Line {\n  a Point\n  b Point\n}\n";
    let initial = [
        ("a.baml", POINT_V1),
        ("b.baml", CONSUMER),
        ("z.baml", UNRELATED),
    ];
    let edited = [("a.baml", a2), ("b.baml", CONSUMER), ("z.baml", UNRELATED)];
    assert_served_equals_honest(&initial, &edited);
}

// ── Scenario 5: layout reorder (field reorder) ───────────────────────────────

#[test]
fn oracle_layout_reorder() {
    let initial = [
        ("a.baml", POINT_V1),
        ("b.baml", CONSUMER),
        ("z.baml", UNRELATED),
    ];
    let edited = [
        ("a.baml", POINT_V2_REORDER),
        ("b.baml", CONSUMER),
        ("z.baml", UNRELATED),
    ];
    let Some(dirty) = assert_served_equals_honest(&initial, &edited) else {
        return;
    };
    assert!(dirty.contains("b.baml"), "field reader dirtied: {dirty:?}");
}

// ── Scenario 6: throws edit — the throws-propagation sentinel ─────────────────

/// A body-only edit grows `risky`'s inferred (transitive) throws without moving
/// its signature, so the caller `guarded` (declared `throws never`) gains a
/// contract-violation error. Served must equal honest — which only holds
/// because the throws-change propagation re-checks `guarded`.
///
/// `risky` deliberately declares no `throws` clause: a declared clause is a
/// *closed* set, so "inferred throws grow while the signature is unchanged" is
/// only expressible through an undeclared-throws function whose set is
/// body-inferred.
const THROWS_ERR: &str = "class MyErr {\n  msg string\n}\n";
const THROWS_BOOM: &str =
    "function boom() -> int throws MyErr {\n  throw MyErr { msg: \"x\" }\n}\n";
const THROWS_RISKY_V1: &str = "function risky() -> int {\n  0\n}\n";
const THROWS_RISKY_V2: &str = "function risky() -> int {\n  boom()\n}\n";
const THROWS_GUARDED: &str = "function guarded() -> int throws never {\n  risky()\n}\n";

#[test]
fn oracle_throws_edit() {
    let initial = [
        ("err.baml", THROWS_ERR),
        ("boom.baml", THROWS_BOOM),
        ("risky.baml", THROWS_RISKY_V1),
        ("guarded.baml", THROWS_GUARDED),
    ];
    let edited = [
        ("err.baml", THROWS_ERR),
        ("boom.baml", THROWS_BOOM),
        ("risky.baml", THROWS_RISKY_V2),
        ("guarded.baml", THROWS_GUARDED),
    ];
    let Some(dirty) = assert_served_equals_honest(&initial, &edited) else {
        return;
    };
    assert!(
        dirty.contains("guarded.baml"),
        "the caller must be re-checked when the callee's throws grow: {dirty:?}"
    );
}

// ── Scenario 7: delete-file ──────────────────────────────────────────────────

#[test]
fn oracle_delete_file() {
    // Deleting the file that defines `Point` makes the consumer's `Point`
    // reference unresolved — served must reproduce the honest error set.
    let initial = [
        ("a.baml", POINT_V1),
        ("b.baml", CONSUMER),
        ("z.baml", UNRELATED),
    ];
    let edited = [("b.baml", CONSUMER), ("z.baml", UNRELATED)];
    assert_served_equals_honest(&initial, &edited);
}

// ── Scenario 8: warning in a clean file survives ─────────────────────────────

#[test]
fn oracle_warning_in_clean_file_is_served() {
    // `w.baml` emits an unreachable-code warning (E0146). Editing an unrelated
    // file must keep `w.baml` clean, yet its warning must still appear in the
    // served set (byte-identical to honest) — served from cache, not dropped.
    let initial = [("w.baml", WARN), ("z.baml", UNRELATED)];
    let edited = [
        ("w.baml", WARN),
        ("z.baml", "function unrelated() -> int {\n  43\n}\n"),
    ];
    let Some(dirty) = run_scenario(&initial, &edited).map(|r| {
        assert_eq!(r.served, r.honest, "served warning must match honest");
        assert!(
            r.served.contains("E0146"),
            "the clean file's unreachable-code warning must be present in the served set:\n{}",
            r.served
        );
        r.dirty
    }) else {
        return;
    };
    assert!(
        !dirty.contains("w.baml"),
        "warning file stays clean: {dirty:?}"
    );
}

// ── Scenario 9: new error in a dirty file still gates ────────────────────────

#[test]
fn oracle_new_error_in_dirty_file() {
    let m1 = "function main() -> int {\n  0\n}\n";
    let m2 = "function main() -> int {\n  \"not an int\"\n}\n";
    let initial = [("a.baml", POINT_V1), ("b.baml", CONSUMER), ("m.baml", m1)];
    let edited = [("a.baml", POINT_V1), ("b.baml", CONSUMER), ("m.baml", m2)];
    let Some(r) = run_scenario(&initial, &edited) else {
        return;
    };
    assert_eq!(r.served, r.honest, "served error must match honest");
    assert!(
        r.served.contains("E0001"),
        "the new type error must gate (appear in the served set):\n{}",
        r.served
    );
}

// ── `baml check`: warning rendering and mixed diagnostic classes ────────────

#[test]
fn check_cold_and_warm_warning_output_is_byte_identical() {
    let files = [("w.baml", WARN), ("z.baml", UNRELATED)];
    let Some(result) = run_check_scenario(&files, &files) else {
        return;
    };
    assert_eq!(result.served, result.honest);
    assert!(
        result.served.contains("E0146"),
        "the cached clean-file warning must still be rendered:\n{}",
        result.served
    );
}

#[test]
fn check_merges_clean_warning_dirty_error_and_cross_file_conflict() {
    let stable = "function duplicate() -> int {\n  1\n}\n";
    let initial_dirty = "function distinct() -> int {\n  2\n}\n";
    let edited_dirty = "function duplicate() -> int {\n  \"not an int\"\n}\n";
    let initial = [
        ("a.baml", stable),
        ("d.baml", initial_dirty),
        ("w.baml", WARN),
    ];
    let edited = [
        ("a.baml", stable),
        ("d.baml", edited_dirty),
        ("w.baml", WARN),
    ];
    let Some(result) = run_check_scenario(&initial, &edited) else {
        return;
    };
    assert_eq!(
        result.served, result.honest,
        "incremental check output must be byte-identical to an honest check"
    );
    assert!(result.served.contains("E0146"), "clean warning missing");
    assert!(result.served.contains("E0001"), "dirty-file error missing");
    assert!(
        result.served.contains("duplicate function definition"),
        "cross-file conflict missing:\n{}",
        result.served
    );
}

#[test]
fn check_corrupt_clean_blob_degrades_to_honest_file_check() {
    if cache_disabled() {
        return;
    }
    with_stored_manifest(&[("w.baml", WARN), ("z.baml", UNRELATED)], |ctx, db| {
        ctx.corrupt_manifest_diagnostics_for_test("w.baml");
        let pending_plan = ctx.plan_reuse(db);
        let plan = prepare_reuse_plan(db, pending_plan);
        let served = ctx.collect_diagnostics_for_check(db, plan.as_ref());
        let honest = baml_project::collect_compiler2_diagnostics(db);
        assert_eq!(
            render_project_diagnostics(db, &served),
            render_project_diagnostics(db, &honest),
            "an undecodable clean-file blob must be recomputed honestly"
        );
    });
}

// ── Layout-scoped sentinel in mixed class+function files ─────────────────────

// A file that defines a class AND a free function; a layout-baker naming
// nothing it defines.
const MIXED_SIG_V1: &str =
    "class Widget {\n  w int\n  h int\n}\nfunction helper(a: int) -> int {\n  a\n}\n";
const MIXED_SIG_V2: &str =
    "class Widget {\n  w int\n  h int\n}\nfunction helper(a: int, b: int) -> int {\n  a + b\n}\n";
const MIXED_REORDER: &str =
    "class Widget {\n  h int\n  w int\n}\nfunction helper(a: int) -> int {\n  a\n}\n";
const LAYOUT_BAKER: &str =
    "class Other {\n  a int\n  b int\n}\nfunction reado(o: Other) -> int {\n  o.a\n}\n";

#[test]
fn oracle_mixed_file_function_sig_edit() {
    // A function-only signature edit in a class-defining file must not fire the
    // layout sentinel: the served (incremental) diagnostics still match the
    // honest full check, and the layout-baker stays clean.
    let initial = [
        ("mixed.baml", MIXED_SIG_V1),
        ("baker.baml", LAYOUT_BAKER),
        ("z.baml", UNRELATED),
    ];
    let edited = [
        ("mixed.baml", MIXED_SIG_V2),
        ("baker.baml", LAYOUT_BAKER),
        ("z.baml", UNRELATED),
    ];
    let Some(dirty) = assert_served_equals_honest(&initial, &edited) else {
        return;
    };
    assert!(
        !dirty.contains("baker.baml"),
        "the layout-baker must stay clean on a function-only sig edit: {dirty:?}"
    );
}

#[test]
fn oracle_mixed_file_field_reorder() {
    // A field reorder in the same mixed file fires the sentinel (dirtying the
    // layout-baker); served must still equal honest.
    let initial = [
        ("mixed.baml", MIXED_SIG_V1),
        ("baker.baml", LAYOUT_BAKER),
        ("z.baml", UNRELATED),
    ];
    let edited = [
        ("mixed.baml", MIXED_REORDER),
        ("baker.baml", LAYOUT_BAKER),
        ("z.baml", UNRELATED),
    ];
    let Some(dirty) = assert_served_equals_honest(&initial, &edited) else {
        return;
    };
    assert!(
        dirty.contains("baker.baml"),
        "a field reorder must fire the sentinel and dirty the layout-baker: {dirty:?}"
    );
}

// ── Verify oracle: passes on a faithful cache, bails on a stale one ──────────

/// Store a manifest for `files` (a warning-bearing clean file), then hand the
/// reopened context + a fresh database to `check`. Uses the env-independent core
/// so no `BAML_CACHE_VERIFY` mutation is needed (parallel-test safe).
fn with_stored_manifest(
    files: &[(&str, &str)],
    check: impl FnOnce(&CacheContext, &mut ProjectDatabase),
) {
    let root = oracle_root();
    let _ = compile_and_store_v1(&root, files);

    let r1 = resolved(&root, files);
    let mut db2 = project_load::build_db_from_sources(&r1, |_| {});
    let ctx2 = CacheContext::open(&r1, false).expect("cache reopens");
    check(&ctx2, &mut db2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn verify_diagnostics_passes_for_faithful_cache() {
    if cache_disabled() {
        return;
    }
    with_stored_manifest(&[("w.baml", WARN), ("z.baml", UNRELATED)], |ctx, db| {
        assert!(
            ctx.check_cached_diagnostics_against_fresh(db).is_ok(),
            "the oracle must not bail on a faithfully-cached clean file"
        );
    });
}

#[test]
fn verify_diagnostics_bails_on_a_stale_cache() {
    if cache_disabled() {
        return;
    }
    // The stored file's SOURCE is a warning, but its content_hash still matches
    // (unchanged), so the oracle would serve the cached blob. If the cache is
    // empty (a stale substitute that dropped the warning) while a fresh
    // check_file still produces it, the oracle must bail.
    with_stored_manifest(&[("w.baml", WARN), ("z.baml", UNRELATED)], |ctx, db| {
        // Overwrite the manifest so w.baml's cached diagnostics are empty (a
        // stale serve) while its content is unchanged.
        ctx.poison_manifest_diagnostics_for_test("w.baml");
        assert!(
            ctx.check_cached_diagnostics_against_fresh(db).is_err(),
            "the oracle must bail when the cached diagnostics drop a warning the fresh check has"
        );
    });
}
