//! Centralized diagnostic collection for BAML projects.
//!
//! This module provides the `check()` and `check_file()` methods that collect
//! all diagnostics from a BAML project using the unified `Diagnostic` type.
//!
//! ## Example
//!
//! ```ignore
//! let result = db.check();
//! for diag in &result.diagnostics {
//!     println!("{}", diag.message);
//! }
//! ```

use std::collections::HashMap;

use baml_compiler_diagnostics::Diagnostic;
use baml_db::{FileId, SourceFile};
use baml_lsp2_actions::check_file as lsp2_check_file;

use crate::ProjectDatabase;

/// Result of checking a project, containing diagnostics and metadata for rendering.
#[derive(Debug)]
pub struct CheckResult {
    /// The collected diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Maps `FileId` to source text for diagnostic rendering.
    pub sources: HashMap<FileId, String>,
    /// Maps `FileId` to file path (for URL generation).
    pub file_paths: HashMap<FileId, std::path::PathBuf>,
}

/// Collect all diagnostics from a project using the compiler2 pipeline.
///
/// This replaces the legacy `collect_diagnostics` that used `baml_compiler_hir` /
/// `baml_compiler_tir`. All diagnostics now come from the compiler2 pipeline via
/// `baml_lsp2_actions::check_file`.
///
/// Diagnostics are ALWAYS collected over the full project: the compiler2 pipeline
/// derives the file set internally from [`baml_compiler2_hir::compiler2_all_files`]
/// and checks every file.
///
/// Per-file narrowing is deliberately not done yet. Callers that hold a per-file
/// "dirty set" (e.g. from the bytecode-cache reuse plan) must NOT expect this
/// function to honor it as a filter. Per-file diagnostics invalidation is not yet
/// proven complete — narrowing the checked set to only the dirty files would risk
/// surfacing stale diagnostics for clean files that transitively depend on a
/// changed signature. Until that invalidation is proven sound, every file is
/// re-diagnosed on every call.
pub fn collect_diagnostics(db: &ProjectDatabase) -> Vec<Diagnostic> {
    collect_compiler2_diagnostics(db)
}

/// Collect all diagnostics from the **compiler2** pipeline (parse + HIR2 + TIR2).
///
/// Files checked are [`baml_compiler2_hir::compiler2_all_files`]: user project
/// sources plus compiler2 stdlib stubs under `<builtin>/baml/...` (packages
/// `baml`, `env`, etc.).
///
/// Diagnostics are sorted by (`file_id`, primary span start, message) for
/// stable snapshot output.
pub fn collect_compiler2_diagnostics(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let source_files = baml_compiler2_hir::compiler2_all_files(db);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    // Parallel by default; tiny projects skip the pool dispatch overhead.
    // Thread count follows rayon's global pool (`RAYON_NUM_THREADS` to cap).
    if source_files.len() > 8 {
        collect_file_diagnostics_parallel(db, &source_files, &mut diagnostics);
    } else {
        for file in &source_files {
            diagnostics.extend(lsp2_check_file(db, *file));
        }
    }
    diagnostics.extend(package_level_diagnostics(db, &source_files));
    sort_diagnostics(&mut diagnostics);
    diagnostics
}

/// Run `check_file` for every file across worker threads.
///
/// Output order does not matter — the caller sorts diagnostics by
/// (file, span, message).
fn collect_file_diagnostics_parallel(
    db: &ProjectDatabase,
    source_files: &[SourceFile],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for file_diagnostics in check_files_parallel(db, source_files) {
        diagnostics.extend(file_diagnostics);
    }
}

/// Prime every compiler2 file's PPIR semantic index across worker threads.
///
/// Whole-package aggregate queries (`package_items` / `namespace_items`)
/// fold over **every** file's semantic index, and every file's check demands
/// them. On a cold database, the first worker to claim such an aggregate
/// memo computes parse + lowering + indexing for the entire project inline
/// on its own thread while every other worker parks on that memo's sync
/// slot — the whole compile degenerates to ~1 effective core regardless of
/// thread count. Priming the per-file indexes first keeps all workers busy
/// on file-local work; the aggregate fold itself is then cheap for whichever
/// worker claims it.
///
/// Public because warm cache paths (the serve-time throws gate, package-level
/// diagnostics on a no-op check) demand the same aggregates outside any
/// per-file check fan-out. Priming twice is harmless — the second wave is all
/// memo hits.
pub fn prime_file_indexes_parallel(db: &ProjectDatabase) {
    const CHUNK: usize = 4;
    let all_files = baml_compiler2_hir::compiler2_all_files(db);
    let chunks: Vec<&[SourceFile]> = all_files.chunks(CHUNK).collect();
    let handles: Vec<ProjectDatabase> = chunks.iter().map(|_| db.clone()).collect();
    rayon::scope(move |s| {
        for (chunk, db) in chunks.into_iter().zip(handles) {
            s.spawn(move |_| {
                for file in chunk {
                    let _ = baml_compiler2_ppir::file_semantic_index(&db, *file);
                }
            });
        }
    });
}

/// Check `files` across worker threads, returning each file's diagnostics in
/// input order (so callers that group per file — the LSP's diagnostics
/// candidate — can zip results back to their inputs).
///
/// Every query `check_file` reaches is read-only, and `ProjectDatabase`'s
/// `Clone` produces a shared-storage salsa handle (the rust-analyzer
/// concurrency model): workers share one memo table, so a scope inferred by
/// one thread is a cache hit for every other. `ProjectDatabase` is `Send`
/// but deliberately not `Sync` (each salsa handle carries thread-confined
/// query-stack state), so tasks cannot share `&db` — every task MOVES its
/// own cloned handle instead (an Arc bump; all clones share one memo table).
/// Small chunks keep work-stealing effective on rayon's global pool — files
/// vary a lot in check cost — while amortizing the per-task clone.
pub fn check_files_parallel(db: &ProjectDatabase, files: &[SourceFile]) -> Vec<Vec<Diagnostic>> {
    const CHUNK: usize = 4;

    prime_file_indexes_parallel(db);

    let chunks: Vec<(usize, &[SourceFile])> = files.chunks(CHUNK).enumerate().collect();
    // Handles are cloned OUTSIDE the rayon scope: a `!Sync` database can't be
    // borrowed by the (Send) scope closure, so each chunk's handle is created
    // up front and MOVED into its task.
    let handles: Vec<ProjectDatabase> = chunks.iter().map(|_| db.clone()).collect();
    let (tx, rx) = std::sync::mpsc::channel::<(usize, Vec<Vec<Diagnostic>>)>();
    rayon::scope(move |s| {
        for ((chunk_index, chunk), db) in chunks.into_iter().zip(handles) {
            let tx = tx.clone();
            s.spawn(move |_| {
                let out: Vec<Vec<Diagnostic>> = chunk
                    .iter()
                    .map(|file| lsp2_check_file(&db, *file))
                    .collect();
                // Receiver outlives the scope; a send only fails if it
                // dropped early, which would mean a panic elsewhere.
                let _ = tx.send((chunk_index, out));
            });
        }
    });
    let mut slots: Vec<Option<Vec<Diagnostic>>> = (0..files.len()).map(|_| None).collect();
    for (chunk_index, out) in rx {
        for (offset, file_diagnostics) in out.into_iter().enumerate() {
            slots[chunk_index * CHUNK + offset] = Some(file_diagnostics);
        }
    }
    slots.into_iter().map(Option::unwrap_or_default).collect()
}

/// The per-checked-file split alongside the merged, honest-ordered set produced
/// by [`collect_compiler2_diagnostics_narrowed`].
pub struct NarrowedDiagnostics {
    /// Merged set: freshly-checked files' diagnostics, the caller-supplied
    /// clean-file `precomputed`, and the always-recomputed package-level
    /// diagnostics — sorted by the same comparator as the honest collector, so
    /// it renders byte-identically to a full run.
    pub merged: Vec<Diagnostic>,
    /// Only the diagnostics `check_file` freshly produced for the checked
    /// files. Excludes `precomputed` and the package-level set, so it is exactly
    /// what the caller persists per dirty file (the package-level set is never
    /// cached — it is recomputed every compile).
    pub fresh: Vec<Diagnostic>,
}

/// Like [`collect_compiler2_diagnostics`], but run `check_file` only for files
/// where `should_check(file)` is true and fold in `precomputed` — diagnostics
/// for the skipped (clean) files, already rehydrated with current-process
/// `FileId`s. Builtins must be forced through `should_check` by the caller
/// (they never appear in the manifest / clean set). Package-level
/// conflicts/shadows are always recomputed. The final sort re-runs on
/// current-process `FileId`s, so the merged set renders byte-identically to the
/// honest collector when `should_check` is all-true and `precomputed` is empty.
pub fn collect_compiler2_diagnostics_narrowed(
    db: &ProjectDatabase,
    should_check: &dyn Fn(SourceFile) -> bool,
    precomputed: Vec<Diagnostic>,
) -> NarrowedDiagnostics {
    let source_files = baml_compiler2_hir::compiler2_all_files(db);
    // Even a fully-served (zero-checked-files) collection ends in
    // `package_level_diagnostics`, which derives the whole-package aggregates
    // — prime the per-file indexes across workers so that derivation is a
    // parallel-fed fold, not a serial parse of the project. When the caller
    // already primed (cache gate, cold fan-out) this is all memo hits.
    prime_file_indexes_parallel(db);
    // Filter up front (outside any parallel region): `should_check` is a plain
    // `&dyn Fn`, so it never has to be thread-safe.
    let checked: Vec<SourceFile> = source_files
        .iter()
        .copied()
        .filter(|file| should_check(*file))
        .collect();
    let mut fresh: Vec<Diagnostic> = Vec::new();
    // Same parallel-by-default policy as [`collect_compiler2_diagnostics`]. On
    // a cold check `should_check` is all-true and `checked` is the whole
    // project, so a serial loop here would leave every core but one idle; on a
    // warm incremental check the dirty set is small and stays on the serial
    // arm. Cross-file collection order is not part of the contract: `merged`
    // gets the total-order sort below, and `fresh` consumers group by owner
    // file (per-file order is preserved — each file's diagnostics come from
    // one `lsp2_check_file` call, appended contiguously).
    if checked.len() > 8 {
        collect_file_diagnostics_parallel(db, &checked, &mut fresh);
    } else {
        for file in &checked {
            fresh.extend(lsp2_check_file(db, *file));
        }
    }
    let mut merged = fresh.clone();
    merged.extend(precomputed);
    merged.extend(package_level_diagnostics(db, &source_files));
    sort_diagnostics(&mut merged);
    NarrowedDiagnostics { merged, fresh }
}

/// Public wrapper over the private `package_level_diagnostics` for callers that assemble
/// per-file diagnostics themselves (the LSP's candidate builder): these
/// cross-file diagnostics come from `package_items`, not `check_file`, so a
/// per-file sweep alone silently misses them.
pub fn collect_package_level_diagnostics(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let source_files = baml_compiler2_hir::compiler2_all_files(db);
    package_level_diagnostics(db, &source_files)
}

/// Package-level diagnostics (cross-file name conflicts and namespace shadows),
/// emitted outside `check_file` and therefore recomputed on every compile —
/// never served from the per-file diagnostics cache.
fn package_level_diagnostics(db: &ProjectDatabase, source_files: &[SourceFile]) -> Vec<Diagnostic> {
    let mut seen_packages = std::collections::HashSet::new();
    for file in source_files {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        seen_packages.insert(pkg_info.package.clone());
    }
    let mut diagnostics = Vec::new();
    for pkg_name in seen_packages {
        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_name);
        let items = baml_compiler2_hir::package::package_items(db, pkg_id);
        for conflict in items.conflicts() {
            diagnostics.push(conflict.to_diagnostic(db));
        }
        for shadow in items.shadows() {
            diagnostics.push(shadow.to_diagnostic(db));
        }
    }
    diagnostics
}

/// Total, deterministic diagnostic ordering.
///
/// The primary key is unchanged — (`file_id`, primary span start, message) —
/// so any set of diagnostics without exact ties renders exactly as before.
/// But parallel check ([`collect_file_diagnostics_parallel`]) collects file
/// chunks in nondeterministic completion order, and a `sort_by` leaves
/// elements that compare `Equal` in their (now-arbitrary) input order. So two
/// *distinct* diagnostics sharing the same file/start/message would render in
/// run-dependent order. The tie-breakers below — span end, then a structural
/// `Debug` encoding that totally covers the remaining fields (id, severity,
/// phase, annotations, related info) — give a total order, guaranteeing
/// byte-identical output regardless of thread scheduling. They only run on
/// exact (file, start, message) ties, so non-tied output is untouched and the
/// `Debug` formatting cost is not paid in the common case.
fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    use std::cmp::Ordering;
    diagnostics.sort_by(|a, b| {
        let a_span = a.primary_span();
        let b_span = b.primary_span();
        let primary = match (a_span, b_span) {
            (Some(sa), Some(sb)) => sa
                .file_id
                .as_u32()
                .cmp(&sb.file_id.as_u32())
                .then_with(|| sa.range.start().cmp(&sb.range.start()))
                .then_with(|| a.message.cmp(&b.message)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.message.cmp(&b.message),
        };
        primary
            .then_with(|| match (a_span, b_span) {
                (Some(sa), Some(sb)) => sa.range.end().cmp(&sb.range.end()),
                _ => Ordering::Equal,
            })
            // Structural catch-all: a stable, total encoding of every remaining
            // field, so distinct diagnostics never compare Equal.
            .then_with(|| format!("{a:?}").cmp(&format!("{b:?}")))
    });
}

impl ProjectDatabase {
    /// Check the entire project and return all diagnostics.
    ///
    /// This is the centralized entry point for diagnostic collection, replacing
    /// the duplicated logic in the LSP server and test infrastructure.
    ///
    /// Returns a `CheckResult` containing diagnostics and metadata for rendering.
    pub fn check(&self) -> CheckResult {
        let Some(_project) = self.get_project() else {
            return CheckResult {
                diagnostics: Vec::new(),
                sources: HashMap::new(),
                file_paths: HashMap::new(),
            };
        };

        let source_files: Vec<SourceFile> = self.get_source_files();
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, std::path::PathBuf> = HashMap::new();

        // Build all maps from user files
        for source_file in &source_files {
            let file_id = source_file.file_id(self);
            let text = source_file.text(self).clone();
            let path = source_file.path(self);

            sources.insert(file_id, text);
            file_paths.insert(file_id, path);
        }

        // Also register compiler2 builtin files for diagnostics rendering
        let all_c2_files = baml_compiler2_hir::compiler2_all_files(self);
        for file in &all_c2_files {
            let file_id = file.file_id(self);
            if let std::collections::hash_map::Entry::Vacant(e) = sources.entry(file_id) {
                e.insert(file.text(self).clone());
                file_paths.insert(file_id, file.path(self));
            }
        }

        let diagnostics = collect_compiler2_diagnostics(self);

        CheckResult {
            diagnostics,
            sources,
            file_paths,
        }
    }

    /// Check a single file and return diagnostics for that file only.
    ///
    /// Note: This still requires the full project context for type checking.
    pub fn check_file(&self, file: SourceFile) -> Vec<Diagnostic> {
        lsp2_check_file(self, file)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity};
    use baml_db::{FileId, Span};
    use text_size::TextRange;

    use super::*;

    /// `sort_diagnostics` must impose a total order: diagnostics tied on the
    /// primary key (file, start, message) but differing elsewhere must sort
    /// into the same sequence regardless of input order, or parallel check
    /// (which produces them in nondeterministic completion order) would render
    /// nondeterministically.
    #[test]
    fn sort_diagnostics_is_a_total_order_over_ties() {
        let at = |start: u32, end: u32| Span {
            file_id: FileId::new(0),
            range: TextRange::new(start.into(), end.into()),
        };
        // All four share (file 0, start 10, message "dup") — the entire primary
        // key — but differ in span end, id, severity, and phase.
        let make = |id, sev, end, phase| {
            let mut d = Diagnostic::new(id, sev, "dup");
            d = d.with_primary_span(at(10, end)).with_phase(phase);
            d
        };
        let originals = vec![
            make(
                DiagnosticId::TypeMismatch,
                Severity::Error,
                20,
                DiagnosticPhase::Type,
            ),
            make(
                DiagnosticId::UnknownType,
                Severity::Warning,
                15,
                DiagnosticPhase::Parse,
            ),
            make(
                DiagnosticId::TypeMismatch,
                Severity::Warning,
                20,
                DiagnosticPhase::Type,
            ),
            make(
                DiagnosticId::UnknownVariable,
                Severity::Error,
                20,
                DiagnosticPhase::Hir,
            ),
        ];

        // Sorting any permutation must yield the identical sequence.
        let mut canonical = originals.clone();
        sort_diagnostics(&mut canonical);
        for rotate in 0..originals.len() {
            let mut perm = originals.clone();
            perm.rotate_left(rotate);
            perm.reverse();
            sort_diagnostics(&mut perm);
            assert_eq!(
                format!("{perm:?}"),
                format!("{canonical:?}"),
                "sort is not a total order: permutation (rotate {rotate}, reversed) diverged",
            );
        }
        // And the span-end tie-breaker actually ran (the end=15 one sorts first).
        assert_eq!(canonical[0].primary_span().unwrap().range.end(), 15.into());
    }

    #[test]
    #[ignore = "compiler2: llm_types.baml builtin causes unreachable arm errors from catch expressions"]
    fn test_check_empty_project() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    #[ignore = "compiler2: llm_types.baml builtin causes unreachable arm errors from catch expressions"]
    fn test_check_valid_file() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {\n  name string\n}");

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_parse_error() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {");

        let result = db.check();
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn narrowed_all_dirty_equals_honest() {
        // The oracle floor: with `should_check` all-true and no precomputed
        // clean-file set, the narrowed collector must reproduce the honest
        // collector's merged set exactly (same diagnostics, same order).
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/narrow-eq"));
        db.add_or_update_file(
            Path::new("/narrow-eq/a.baml"),
            "class Foo {\n  x int\n}\nfunction bad() -> int {\n  \"nope\"\n}\n",
        );
        db.add_or_update_file(
            Path::new("/narrow-eq/b.baml"),
            "function ok() -> int {\n  1\n}\n",
        );

        let honest = collect_compiler2_diagnostics(&db);
        let narrowed = collect_compiler2_diagnostics_narrowed(&db, &|_| true, Vec::new());
        assert_eq!(
            honest, narrowed.merged,
            "narrowed all-true must equal honest"
        );
        // `fresh` is exactly `merged` minus the always-recomputed package-level
        // set (with `precomputed` empty here): removing the package-level
        // diagnostics from the merged set must leave precisely the fresh set.
        // This pins that the narrowed collector neither double-counts a
        // check_file diagnostic into `fresh` nor leaks a package-level one there.
        let source_files = baml_compiler2_hir::compiler2_all_files(&db);
        let package_level = package_level_diagnostics(&db, &source_files);
        assert_eq!(
            narrowed.merged.len(),
            narrowed.fresh.len() + package_level.len(),
            "merged = fresh + package-level (precomputed empty)"
        );
        let mut fresh_sorted = narrowed.fresh;
        sort_diagnostics(&mut fresh_sorted);
        let mut merged_minus_pkg = narrowed.merged;
        for pkg in &package_level {
            let pos = merged_minus_pkg
                .iter()
                .position(|d| d == pkg)
                .expect("each package-level diagnostic appears in merged");
            merged_minus_pkg.remove(pos);
        }
        assert_eq!(
            fresh_sorted, merged_minus_pkg,
            "fresh must equal merged with the package-level diagnostics removed"
        );
    }
}
