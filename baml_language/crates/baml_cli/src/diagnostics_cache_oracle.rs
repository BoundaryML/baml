//! Oracle for the per-file diagnostics cache (Phase 1): on a warm incremental
//! compile the *served* diagnostics — clean files from cache, dirty files
//! freshly checked — must render byte-identically to an honest full check of
//! the edited sources.
//!
//! Each scenario compiles+stores `v1`, edits to `v2`, then compares the served
//! set (`collect_diagnostics_incremental` through the reuse plan) against the
//! honest set (`collect_compiler2_diagnostics` on an independent fresh
//! database). The scenario matrix mirrors `relink_oracle` — body edit, signature
//! edit, add-fn, add-class, layout reorder, throws edit, delete-file — plus the
//! two Phase-1 cases: a warning in a clean file must survive the incremental
//! compile, and a new error in a dirty file must still gate.
//!
//! These round-trip through the on-disk cache, so (like the `plan_reuse`
//! integration tests) they run only on Linux.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use baml_db::{
    baml_compiler_diagnostics::{Diagnostic, render},
    baml_compiler2_emit::CompileOptions,
};
use baml_project::ProjectDatabase;

use crate::{
    bytecode_cache::{CacheContext, compile_program},
    project_load,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn cache_disabled() -> bool {
    std::env::var_os("BAML_NO_BYTECODE_CACHE").is_some()
        || std::env::var_os("BAML_CACHE_VERIFY").is_some()
}

fn unique_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("baml-diag-oracle-{}-{n}", std::process::id()))
}

fn opts() -> CompileOptions {
    CompileOptions {
        emit_test_cases: false,
    }
}

fn resolved(root: &Path, files: &[(&str, &str)]) -> project_load::ResolvedProject {
    project_load::ResolvedProject {
        root: root.to_path_buf(),
        manifest: None,
        files: files
            .iter()
            .map(|(name, content)| (root.join(name), (*content).to_string()))
            .collect(),
    }
}

/// Render a diagnostic set the way the CLI would, resolving every span's file
/// (user files plus builtins, for cross-file spans) against this database.
fn render_all(db: &ProjectDatabase, diags: &[Diagnostic]) -> String {
    let mut sources = HashMap::new();
    let mut paths = HashMap::new();
    for sf in baml_db::baml_compiler2_hir::compiler2_all_files(db) {
        let fid = sf.file_id(db);
        sources
            .entry(fid)
            .or_insert_with(|| sf.text(db).to_string());
        paths.entry(fid).or_insert_with(|| sf.path(db));
    }
    render::render_diagnostics(diags, &sources, &paths, &render::RenderConfig::cli_auto())
}

struct OracleResult {
    served: String,
    honest: String,
    dirty: HashSet<String>,
}

/// Compile+store `initial`, edit to `edited`, and return the rendered served
/// (incremental) vs honest (fresh full) diagnostics plus the dirty file set.
/// `None` when the on-disk cache is disabled or off-Linux (the mechanism is
/// covered platform-independently by the `plan_reuse` unit tests).
fn run_scenario(initial: &[(&str, &str)], edited: &[(&str, &str)]) -> Option<OracleResult> {
    if cache_disabled() || !cfg!(target_os = "linux") {
        return None;
    }
    let root = unique_root();
    let _ = std::fs::remove_dir_all(&root);

    // v1: compile, gate (to compute fresh diagnostics), store manifest.
    let r1 = resolved(&root, initial);
    let db1 = project_load::build_db_from_sources(&r1, |_| {});
    let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
    let program1 = compile_program(&db1, &opts(), Some(&ctx1), None).expect("v1 compiles");
    let fresh1 = ctx1
        .collect_diagnostics_incremental(&db1, None)
        .fresh_by_file;
    ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
        .expect("v1 manifest stored");

    // v2 served path: reuse plan + seed + incremental gate.
    let r2 = resolved(&root, edited);
    let mut db2 = project_load::build_db_from_sources(&r2, |_| {});
    let ctx2 = CacheContext::open(&r2, false).expect("cache reopens");
    let plan = ctx2.plan_reuse(&db2);
    if let Some(p) = &plan {
        db2.set_seeded_throw_facts(p.seeded_throw_facts.clone());
    }
    let served = ctx2
        .collect_diagnostics_incremental(&db2, plan.as_ref())
        .merged;
    let served_render = render_all(&db2, &served);

    let dirty: HashSet<String> = plan
        .as_ref()
        .map(|p| {
            p.dirty_files
                .iter()
                .filter_map(|sf| {
                    sf.path(&db2)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default();

    // v2 honest path: an independent fresh database, no cache, no seed.
    let db_honest = project_load::build_db_from_sources(&r2, |_| {});
    let honest = baml_project::collect_compiler2_diagnostics(&db_honest);
    let honest_render = render_all(&db_honest, &honest);

    let _ = std::fs::remove_dir_all(&root);
    Some(OracleResult {
        served: served_render,
        honest: honest_render,
        dirty,
    })
}

/// Assert served == honest for a scenario; returns the dirty set for extra
/// assertions. A skipped run (disabled cache / non-Linux) returns `None`.
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
/// its `throws _` signature, so the caller `guarded` (declared `throws never`)
/// gains a contract-violation error. Served must equal honest — which only holds
/// because the throws-change propagation re-checks `guarded`. Disabling that
/// propagation makes this fail (proven separately in
/// `throws_sentinel_disabled_breaks_the_oracle`).
const THROWS_ERR: &str = "class MyErr {\n  msg string\n}\n";
const THROWS_BOOM: &str =
    "function boom() -> int throws MyErr {\n  throw MyErr { msg: \"x\" }\n}\n";
const THROWS_RISKY_V1: &str = "function risky() -> int throws _ {\n  0\n}\n";
const THROWS_RISKY_V2: &str = "function risky() -> int throws _ {\n  boom()\n}\n";
const THROWS_GUARDED: &str = "function guarded() -> int throws never {\n  risky()\n}\n";

/// A four-file project fixture (name, content).
type Fixture4 = [(&'static str, &'static str); 4];

fn throws_scenario() -> (Fixture4, Fixture4) {
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
    (initial, edited)
}

#[test]
fn oracle_throws_edit() {
    let (initial, edited) = throws_scenario();
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
    // `w.baml` emits an aliasing warning (E0148). Editing an unrelated file must
    // keep `w.baml` clean, yet its warning must still appear in the served set
    // (byte-identical to honest) — i.e. it is served from cache, not dropped.
    let warn =
        "function warns() -> int {\n  let rows = baml.Array.filled(3, [0]);\n  rows.length()\n}\n";
    let initial = [("w.baml", warn), ("z.baml", UNRELATED)];
    let edited = [
        ("w.baml", warn),
        ("z.baml", "function unrelated() -> int {\n  43\n}\n"),
    ];
    let Some(dirty) = run_scenario(&initial, &edited).map(|r| {
        assert_eq!(r.served, r.honest, "served warning must match honest");
        assert!(
            r.served.contains("E0148"),
            "the clean file's aliasing warning must be present in the served set:\n{}",
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

// ── Phase 2 follow-up: layout-scoped sentinel in mixed class+function files ──

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
    check: impl FnOnce(&CacheContext, &ProjectDatabase),
) {
    let root = unique_root();
    let _ = std::fs::remove_dir_all(&root);
    let r1 = resolved(&root, files);
    let db1 = project_load::build_db_from_sources(&r1, |_| {});
    let ctx1 = CacheContext::open(&r1, false).expect("cache opens");
    let program1 = compile_program(&db1, &opts(), Some(&ctx1), None).expect("compiles");
    let fresh1 = ctx1
        .collect_diagnostics_incremental(&db1, None)
        .fresh_by_file;
    ctx1.store_with_manifest(&db1, &program1, &fresh1, None)
        .expect("manifest stored");

    let db2 = project_load::build_db_from_sources(&r1, |_| {});
    let ctx2 = CacheContext::open(&r1, false).expect("cache reopens");
    check(&ctx2, &db2);
    let _ = std::fs::remove_dir_all(&root);
}

const VERIFY_WARN: &str =
    "function warns() -> int {\n  let rows = baml.Array.filled(3, [0]);\n  rows.length()\n}\n";

#[test]
fn verify_diagnostics_passes_for_faithful_cache() {
    if cache_disabled() || !cfg!(target_os = "linux") {
        return;
    }
    with_stored_manifest(
        &[("w.baml", VERIFY_WARN), ("z.baml", UNRELATED)],
        |ctx, db| {
            assert!(
                ctx.check_cached_diagnostics_against_fresh(db).is_ok(),
                "the oracle must not bail on a faithfully-cached clean file"
            );
        },
    );
}

#[test]
fn verify_diagnostics_bails_on_a_stale_cache() {
    if cache_disabled() || !cfg!(target_os = "linux") {
        return;
    }
    // The stored file's SOURCE is a warning, but its content_hash still matches
    // (unchanged), so the oracle would serve the cached blob. If the cache is
    // empty (a stale substitute that dropped the warning) while a fresh
    // check_file still produces it, the oracle must bail.
    with_stored_manifest(
        &[("w.baml", VERIFY_WARN), ("z.baml", UNRELATED)],
        |ctx, db| {
            // Overwrite the manifest so w.baml's cached diagnostics are empty (a
            // stale serve) while its content is unchanged.
            ctx.poison_manifest_diagnostics_for_test("w.baml");
            assert!(
                ctx.check_cached_diagnostics_against_fresh(db).is_err(),
                "the oracle must bail when the cached diagnostics drop a warning the fresh check has"
            );
        },
    );
}
