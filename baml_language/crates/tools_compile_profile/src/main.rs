//! `tools_compile_profile`: A standalone profiling harness for the BAML compiler.
//!
//! This tool loads a BAML project, runs the full compiler pipeline
//! (parse → HIR → PPIR → TIR → MIR → emit) end-to-end, and reports:
//!
//! - Wall-clock time for each pipeline phase
//! - Per-Salsa-query execution counts (how many times each cached query
//!   actually ran its body)
//! - Per-Salsa-query cache hits (how many times it returned a memoized value)
//! - Aggregate stats grouped by crate prefix
//! - Top-N most executed queries
//! - Top-N queries with lowest cache-hit rate (candidates for redundant work)
//!
//! Design goals:
//! - Fast to run and re-run against any BAML project
//! - No changes to the compiler under test — pure black-box instrumentation
//!   via `salsa::Event` callbacks
//! - Works with external CPU samplers (`samply`, `Instruments`) for
//!   flamegraph analysis of hot Rust code inside individual queries
//!
//! ## Cache mode
//!
//! By default (no `--warm-runs`) every measured run is **cold**: a fresh
//! `ProjectDatabase` is built per run, so Salsa's memoization cache starts
//! empty. This matches how `baml check` is invoked from the CLI (one
//! process, one db). If you want to also measure the effect of Salsa's
//! cache, pass `--warm-runs N` — after each cold run we invoke `check` +
//! `get_bytecode` `N` more times against the *same* db. Only the first
//! (cold) invocation fills the cache; the warm invocations then hit a fully
//! warm query cache, but still pay the uncached wrapper cost of
//! `db.check()` / `db.get_bytecode()` (materialization, walking, cloning)
//! on every call.
//!
//! ## Usage
//!
//! ```text
//! # Human-readable report (cold-only)
//! cargo run --release -p tools_compile_profile -- /path/to/baml/project
//!
//! # Cold run followed by 2 warm re-runs on the same db (measures cache)
//! cargo run --release -p tools_compile_profile -- --warm-runs 2 /path/to/project
//!
//! # JSON output for programmatic diffing
//! cargo run --release -p tools_compile_profile -- --json /path/to/project
//!
//! # Repeat N times (useful for measuring cold-cache variance)
//! cargo run --release -p tools_compile_profile -- --repeat 3 /path/to/project
//!
//! # Skip bytecode generation (measure just `check`, not `check + emit`)
//! cargo run --release -p tools_compile_profile -- --check-only /path/to/project
//!
//! # Combine with a CPU sampler for a flamegraph
//! cargo build --release -p tools_compile_profile
//! samply record ./target/release/tools_compile_profile /path/to/project
//! ```

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use baml_compiler_diagnostics::Severity;

/// The compiler's workload is dominated by small short-lived allocations
/// (`Ty` trees, `Vec`s, `SmolStr`s): macOS system malloc was ~35% of
/// single-threaded cold-compile CPU in `samply` profiles. mimalloc cuts
/// that overhead substantially.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use clap::Parser;
use salsa::{Database, Event, EventKind};

/// Profile a full BAML compile pipeline against a project on disk.
#[derive(Parser, Debug)]
#[command(
    name = "tools_compile_profile",
    about = "Profile a full BAML compile (parse → HIR → PPIR → TIR → MIR → emit) end-to-end."
)]
struct Args {
    /// Path to a BAML project (containing `baml_src/` or `baml.toml`) OR
    /// to a directory of `.baml` files. If a `baml_src/` subdirectory is
    /// present under the given path, its contents are loaded; otherwise
    /// the given directory is walked recursively for `.baml` files.
    project: PathBuf,

    /// Emit machine-readable JSON instead of the human-readable report.
    #[arg(long)]
    json: bool,

    /// Repeat the compile N times back-to-back with a fresh database each
    /// time. Useful for measuring cold-cache variance. The report shows the
    /// median run when N > 1.
    #[arg(long, default_value_t = 1)]
    repeat: usize,

    /// After each cold run, additionally invoke `check` + `get_bytecode` N
    /// more times against the *same* database (no input mutation between
    /// invocations). Every event Salsa fires on these calls is a hit
    /// against its memoization cache, so this measures cache
    /// effectiveness. A warm run that still executes many queries is
    /// evidence of a caching gap.
    #[arg(long, default_value_t = 0)]
    warm_runs: usize,

    /// Only run diagnostics collection (`check`), skip bytecode generation
    /// (`emit`). Approximates what `baml check` does when it exits early on
    /// diagnostic errors.
    #[arg(long)]
    check_only: bool,

    /// Path substring identifying which discovered source file to mutate
    /// before warm run #1, instead of every warm run being a content-free
    /// no-op rerun. A no-op rerun short-circuits at the top-level tracked
    /// query without recursing into any child query, so it fires zero
    /// events (not even cache-hit events) and cannot show real incremental
    /// reuse. Combine with `--edit-find`/`--edit-replace` to apply exactly
    /// one deterministic textual substitution and observe which queries
    /// actually get invalidated (`executed`) vs stay cached (`cache_hits`)
    /// after a real edit — the LSP edit-loop case.
    #[arg(long)]
    edit_file: Option<String>,

    /// Substring to find in the `--edit-file` target (required with
    /// `--edit-file`). Only the first occurrence is replaced.
    #[arg(long)]
    edit_find: Option<String>,

    /// Replacement text for `--edit-find` (required with `--edit-file`).
    #[arg(long)]
    edit_replace: Option<String>,

    /// Show this many entries in each top-N table.
    #[arg(long, default_value_t = 30)]
    top_n: usize,

    /// Additionally emit a compact "phase summary" line at the very end,
    /// suitable for grepping/telemetry across many runs.
    #[arg(long)]
    summary_line: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.project.exists() {
        anyhow::bail!("path does not exist: {}", args.project.display());
    }

    let (root, source_paths) = discover_project_sources(&args.project)?;
    if source_paths.is_empty() {
        anyhow::bail!(
            "no .baml files discovered under {}. Point --project at a directory containing \
             `baml_src/` or `.baml` files.",
            args.project.display()
        );
    }

    let sources = read_sources(&source_paths)?;
    let total_bytes: usize = sources.iter().map(|(_, text)| text.len()).sum();
    let total_lines: usize = sources
        .iter()
        .map(|(_, text)| text.matches('\n').count() + 1)
        .sum();

    eprintln!("[tools_compile_profile] project: {}", root.display());
    eprintln!(
        "[tools_compile_profile] {} files, {} lines, {} bytes",
        sources.len(),
        total_lines,
        total_bytes,
    );
    if args.repeat > 1 {
        eprintln!("[tools_compile_profile] running {} times", args.repeat);
    }

    // Resolve the (index, mutated-text) pair for `--edit-file`/`--edit-find`/
    // `--edit-replace`, if given. Applied once, before warm run #1, to
    // measure real incremental-edit reuse instead of a vacuous no-op rerun.
    let edit: Option<(usize, String)> = if let Some(needle) = args.edit_file.as_deref() {
        let idx = sources
            .iter()
            .position(|(p, _)| p.to_string_lossy().contains(needle))
            .unwrap_or_else(|| {
                panic!("--edit-file {needle:?}: no discovered source path contains this substring")
            });
        let find = args
            .edit_find
            .as_deref()
            .expect("--edit-file requires --edit-find");
        let replace = args
            .edit_replace
            .as_deref()
            .expect("--edit-file requires --edit-replace");
        let (path, original) = &sources[idx];
        if !original.contains(find) {
            panic!(
                "--edit-find {find:?} not found in {}",
                path.to_string_lossy()
            );
        }
        eprintln!(
            "[tools_compile_profile] edit probe: {} — {find:?} -> {replace:?} (applied before warm run #1)",
            path.to_string_lossy()
        );
        Some((idx, original.replacen(find, replace, 1)))
    } else {
        None
    };

    let cold_runs = args.repeat.max(1);
    let mut runs: Vec<RunReport> = Vec::with_capacity(cold_runs * (1 + args.warm_runs));
    for i in 0..cold_runs {
        eprintln!(
            "[tools_compile_profile] cold run {}/{} (fresh database, empty Salsa cache)",
            i + 1,
            cold_runs
        );
        // `run_cold_plus_warm` builds a fresh db (cold), invokes the
        // pipeline once, then invokes it `warm_runs` more times against
        // the *same* db — each of those additional invocations reuses
        // Salsa's cache, so its query counts show cache effectiveness
        // (or, with `edit` set, real incremental-edit reuse after warm #1).
        let reports = run_cold_plus_warm(
            &root,
            &sources,
            args.check_only,
            args.warm_runs,
            edit.as_ref(),
        )?;
        for r in &reports {
            eprintln!(
                "[tools_compile_profile]   [{}] total: {:.3}s  (check {:.3}s, emit {:.3}s, exec {} queries, hit {})",
                r.mode.label(),
                r.total.as_secs_f64(),
                r.check.as_secs_f64(),
                r.emit.as_secs_f64(),
                total_executions(&r.queries),
                total_cache_hits(&r.queries),
            );
        }
        runs.extend(reports);
    }

    // Pick a representative *cold* run: median by total time. Warm runs
    // are usually near-zero and would dominate the "min" bucket if we
    // mixed them together, hiding the real cold-start cost.
    let mut cold_only: Vec<&RunReport> = runs.iter().filter(|r| r.mode.is_cold()).collect();
    cold_only.sort_by_key(|r| r.total);
    let representative = cold_only[cold_only.len() / 2];

    if args.json {
        print_json(
            representative,
            &runs,
            total_bytes,
            total_lines,
            sources.len(),
        );
    } else {
        print_human(
            representative,
            &runs,
            total_bytes,
            total_lines,
            sources.len(),
            args.top_n,
        );
    }

    if args.summary_line {
        print_summary_line(representative, sources.len(), total_lines, total_bytes);
    }

    Ok(())
}

/// Discover the project root and its `.baml` source paths.
///
/// If `path/baml_src` exists, we walk that subtree. Otherwise we walk the
/// path itself. We don't try to parse `baml.toml` — this tool is
/// deliberately permissive so it can be pointed at any directory of BAML
/// files (including test fixtures that don't have a manifest).
fn discover_project_sources(path: &Path) -> Result<(PathBuf, Vec<PathBuf>)> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    let walk_root = if canonical.join("baml_src").is_dir() {
        canonical.join("baml_src")
    } else {
        canonical.clone()
    };
    let files = discover_baml_files(&walk_root);
    Ok((canonical, files))
}

fn read_sources(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        out.push((path.clone(), text));
    }
    Ok(out)
}

/// Whether a `RunReport` corresponds to a cold-cache pipeline
/// invocation (fresh db) or a warm-cache one (re-invoked on the same db
/// without any input changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    /// Fresh `ProjectDatabase`; Salsa memoization cache starts empty.
    /// Mirrors CLI `baml check` which spawns a new process per run.
    Cold,
    /// `n`-th (1-based) re-invocation of the pipeline on the same
    /// database from a previous cold run, with no input mutation in
    /// between. Every query resolve here should be a cache hit if
    /// memoization is doing its job.
    Warm { index: usize },
}

impl RunMode {
    fn is_cold(self) -> bool {
        matches!(self, RunMode::Cold)
    }
    fn label(self) -> String {
        match self {
            RunMode::Cold => "cold".to_string(),
            RunMode::Warm { index } => format!("warm#{index}"),
        }
    }
}

/// A single compile run's measurements.
#[derive(Debug)]
struct RunReport {
    /// Cold (fresh db) or warm (re-invoked on same db).
    mode: RunMode,
    /// Time to build the database and add all files as Salsa inputs.
    /// Zero for warm runs (the db already exists).
    db_build: Duration,
    /// Time to run `db.check()` (all diagnostics).
    check: Duration,
    /// Time to run `db.get_bytecode()` (parse → MIR → emit). Zero when
    /// `--check-only` was passed OR when `check()` surfaced errors and
    /// bytecode generation was skipped.
    emit: Duration,
    /// Total wall clock (`db_build + check + emit`).
    total: Duration,
    /// Number of error-severity diagnostics produced by `check()`.
    error_count: usize,
    /// Number of warning-severity diagnostics produced by `check()`.
    warning_count: usize,
    /// Whether `get_bytecode()` was called (i.e., not `--check-only` and
    /// no errors from `check`).
    emit_attempted: bool,
    /// Number of times Salsa reported a fixpoint cycle iteration during
    /// the run. Rare; large numbers hint at a query that's oscillating.
    cycle_iterations: u64,
    /// Per-query stats resolved to human-readable names, sorted by
    /// `executed` descending. Deterministic across runs.
    queries: Vec<QueryRow>,
}

/// One row in the per-query event table.
#[derive(Debug, Clone, serde::Serialize)]
struct QueryRow {
    /// The Salsa ingredient's debug name, e.g.
    /// `baml_compiler2_hir::file_semantic_index::file_semantic_index`. This
    /// is Salsa's own `Ingredient::debug_name`, so it always matches what
    /// Salsa's tracing prints.
    name: String,
    /// Times Salsa actually ran the query's body (cache miss / stale
    /// input). This is the "real work" number.
    executed: u64,
    /// Times Salsa was able to reuse a memoized value (cache hit).
    cache_hits: u64,
    /// Times we blocked waiting for another thread to compute the same
    /// query. Non-zero here means the compile is trying to be parallel.
    blocked: u64,
}

fn total_executions(rows: &[QueryRow]) -> u64 {
    rows.iter().map(|r| r.executed).sum()
}

fn total_cache_hits(rows: &[QueryRow]) -> u64 {
    rows.iter().map(|r| r.cache_hits).sum()
}

/// Raw per-ingredient counters accumulated on the Salsa event callback.
/// Kept name-free (`IngredientIndex` only) so the callback stays allocation-
/// free on the hot path — name resolution happens once at report time.
#[derive(Default, Debug)]
struct RawStats {
    /// Map from `IngredientIndex` → (executed, cache_hits, blocked). The
    /// `IngredientIndex` newtype is `Hash + Eq + Copy`, so it's a fine
    /// HashMap key without any conversion.
    counters: HashMap<salsa::IngredientIndex, RawCounters>,
}

#[derive(Debug, Default, Clone, Copy)]
struct RawCounters {
    executed: u64,
    cache_hits: u64,
    blocked: u64,
}

/// Build a fresh instrumented database, then invoke the compile pipeline
/// `1 + warm_runs` times: once cold (empty cache) and then `warm_runs`
/// more times against the *same* database. With `edit` unset, no input
/// mutation happens between invocations, so every resolve should be a
/// Salsa cache hit (in practice: a no-op rerun short-circuits at the
/// top-level tracked query and fires ZERO events, not even cache hits —
/// it never even recurses into child queries to check them). With `edit`
/// set to `(source_index, mutated_text)`, that one file is mutated via
/// `add_or_update_file` immediately before warm run #1, so that run's
/// counts show REAL incremental-edit reuse: which queries actually got
/// invalidated (`executed`) vs stayed cached (`cache_hits`) in response to
/// one real content change, matching the LSP edit-loop case.
///
/// The Salsa event callback is installed once on the database and
/// remains active for all invocations. Between invocations we snapshot
/// and clear the per-ingredient counters so each `RunReport` describes
/// only its own invocation's events.
fn run_cold_plus_warm(
    root: &Path,
    sources: &[(PathBuf, String)],
    check_only: bool,
    warm_runs: usize,
    edit: Option<&(usize, String)>,
) -> Result<Vec<RunReport>> {
    let stats: Arc<Mutex<RawStats>> = Arc::new(Mutex::new(RawStats::default()));
    // Cycle events don't need per-query breakdown — a single counter is
    // enough. Keeping it as an `AtomicU64` avoids competing with the
    // per-event mutex for the (very rare) cycle path.
    let cycle_iters = Arc::new(AtomicU64::new(0));

    let stats_cb = Arc::clone(&stats);
    let cycles_cb = Arc::clone(&cycle_iters);
    let callback: baml_project::EventCallback = Box::new(move |event: Event| {
        // We deliberately do NOT resolve query names inside the callback.
        // Salsa's `ingredient_debug_name` allocates a `Cow<str>` per call;
        // doing that on every event would swamp the very compile-time
        // signal we're trying to capture. Instead we accumulate into an
        // `IngredientIndex`-keyed map and resolve names once at report
        // time (after the run completes).
        match event.kind {
            EventKind::WillExecute { database_key } => {
                let idx = database_key.ingredient_index();
                let mut guard = stats_cb.lock().unwrap();
                guard.counters.entry(idx).or_default().executed += 1;
            }
            EventKind::DidValidateMemoizedValue { database_key } => {
                let idx = database_key.ingredient_index();
                let mut guard = stats_cb.lock().unwrap();
                guard.counters.entry(idx).or_default().cache_hits += 1;
            }
            EventKind::WillBlockOn { database_key, .. } => {
                let idx = database_key.ingredient_index();
                let mut guard = stats_cb.lock().unwrap();
                guard.counters.entry(idx).or_default().blocked += 1;
            }
            EventKind::WillIterateCycle { .. } => {
                cycles_cb.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    });

    let build_start = Instant::now();
    let mut db = ProjectDatabase::new_with_event_callback(callback);
    db.set_project_root(root);
    for (path, text) in sources {
        db.add_or_update_file(path, text);
    }
    let db_build = build_start.elapsed();
    // Cold-run counter snapshot baseline is empty — anything the callback
    // recorded during `set_project_root` / `add_or_update_file` (there
    // shouldn't be any query executions yet, but be defensive) gets
    // dropped so we only report events from `check`/`emit`.
    *stats.lock().unwrap() = RawStats::default();
    let cold_cycles_before = cycle_iters.load(Ordering::Relaxed);

    let mut reports = Vec::with_capacity(1 + warm_runs);

    let cold = invoke_pipeline(
        &mut db,
        &stats,
        &cycle_iters,
        cold_cycles_before,
        check_only,
        RunMode::Cold,
        db_build,
    )?;
    let cold_had_errors = cold.error_count > 0;
    reports.push(cold);

    // Warm re-runs share the db. We don't rebuild inputs; Salsa should
    // resolve everything from its cache. If `check_only` was set, we
    // reuse it. If cold had errors, we still exercise the warm path
    // (the caller may want to see whether reruns are also fast even in
    // that case — they should be).
    for i in 1..=warm_runs {
        if i == 1
            && let Some((source_idx, mutated_text)) = edit
        {
            let (path, _original) = &sources[*source_idx];
            db.add_or_update_file(path, mutated_text);
        }
        let cycles_before = cycle_iters.load(Ordering::Relaxed);
        let warm = invoke_pipeline(
            &mut db,
            &stats,
            &cycle_iters,
            cycles_before,
            check_only || cold_had_errors,
            RunMode::Warm { index: i },
            Duration::ZERO,
        )?;
        reports.push(warm);
    }

    Ok(reports)
}

/// Invoke `check` + optionally `get_bytecode` on an already-built db,
/// snapshotting and clearing the per-ingredient counters so this run's
/// `RunReport` describes only its own events. The caller supplies
/// `db_build_time` (nonzero only on cold runs).
fn invoke_pipeline(
    db: &mut ProjectDatabase,
    stats: &Mutex<RawStats>,
    cycle_iters: &AtomicU64,
    cycles_before: u64,
    check_only: bool,
    mode: RunMode,
    db_build_time: Duration,
) -> Result<RunReport> {
    let check_start = Instant::now();
    let check_result = db.check();
    let check = check_start.elapsed();

    // `check` runs the pipeline over *all* files (user sources + compiler2
    // builtin stubs), so its diagnostics can be anchored to builtin files.
    // `get_bytecode()`, like the CLI, only aborts emit on *user-file* errors —
    // builtin-stub diagnostics never block codegen. Mirror that filter for the
    // emit gate so a project whose only errors live in builtin files still
    // emits (and gets measured), instead of being skipped by an over-broad
    // "any error" check.
    let db_ref: &ProjectDatabase = &*db;
    let user_file_ids: std::collections::HashSet<_> = db_ref
        .get_source_files()
        .iter()
        .map(|f| f.file_id(db_ref))
        .collect();
    let user_error_count = check_result
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == Severity::Error
                && d.file_id()
                    .map(|id| user_file_ids.contains(&id))
                    .unwrap_or(false)
        })
        .count();
    // Total counts are still reported as-is (they describe everything `check`
    // found); only the emit gate uses the user-file-filtered count.
    let error_count = check_result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warning_count = check_result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    drop(check_result); // release large HashMaps early

    let (emit, emit_attempted) = if check_only {
        (Duration::ZERO, false)
    } else if user_error_count > 0 {
        // Matches the CLI / `get_bytecode()`: user-file `check` errors abort
        // the pipeline before bytecode generation.
        eprintln!(
            "[tools_compile_profile]   [{}] skipping emit: {user_error_count} user-file error(s) reported by check",
            mode.label()
        );
        (Duration::ZERO, false)
    } else {
        let emit_start = Instant::now();
        db.get_bytecode()
            .map_err(|e| anyhow::anyhow!("bytecode generation failed: {e:?}"))?;
        (emit_start.elapsed(), true)
    };

    // Snapshot and clear the per-run counters. This is what makes
    // per-invocation warm-run reporting possible: each report describes
    // only the events fired during its own `check` + `get_bytecode`.
    // Safe because after those calls return, no more Salsa events can
    // arrive on this thread until the next invocation.
    let raw = std::mem::take(&mut *stats.lock().unwrap());
    let queries = resolve_query_names(db, &raw);

    let cycles_after = cycle_iters.load(Ordering::Relaxed);
    let total = db_build_time + check + emit;

    Ok(RunReport {
        mode,
        db_build: db_build_time,
        check,
        emit,
        total,
        error_count,
        warning_count,
        emit_attempted,
        cycle_iterations: cycles_after - cycles_before,
        queries,
    })
}

/// Look up each ingredient index's debug name and produce a sorted
/// `QueryRow` list. Sort order: `executed desc, name asc` — deterministic
/// so JSON output is diff-friendly across runs.
///
/// Salsa's `Ingredient::debug_name` returns just the function name (e.g.
/// `"file_semantic_index"`), not a qualified path. Different crates can
/// register queries with the same name (e.g. HIR and PPIR both expose
/// `file_semantic_index`) and they will collide in the report. We
/// disambiguate by appending the ingredient index whenever the same name
/// is emitted by more than one ingredient — enough for the reader to
/// tell them apart in the report and keep counts correct.
fn resolve_query_names(db: &ProjectDatabase, raw: &RawStats) -> Vec<QueryRow> {
    let mut name_by_idx: HashMap<salsa::IngredientIndex, String> = raw
        .counters
        .keys()
        .map(|idx| {
            (
                *idx,
                (db as &dyn Database)
                    .ingredient_debug_name(*idx)
                    .to_string(),
            )
        })
        .collect();

    // Detect name collisions: multiple ingredient indices share the same
    // debug name. Suffix the collisions so the report shows each row on
    // its own line with a stable-per-index label.
    let mut count_by_name: HashMap<String, usize> = HashMap::new();
    for name in name_by_idx.values() {
        *count_by_name.entry(name.clone()).or_default() += 1;
    }
    for (idx, name) in &mut name_by_idx {
        if count_by_name.get(name).copied().unwrap_or(0) > 1 {
            // Ingredient index is a `u32`-newtype but its accessor is
            // pub(crate); its `Debug` prints as `IngredientIndex(N)`,
            // which is what we surface to the report.
            *name = format!("{name} [{idx:?}]");
        }
    }

    let mut rows: Vec<QueryRow> = raw
        .counters
        .iter()
        .map(|(idx, c)| QueryRow {
            name: name_by_idx
                .remove(idx)
                .unwrap_or_else(|| format!("{idx:?}")),
            executed: c.executed,
            cache_hits: c.cache_hits,
            blocked: c.blocked,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.executed
            .cmp(&a.executed)
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

// ── Reporting ────────────────────────────────────────────────────────────

/// Group a query into a coarse pipeline phase.
///
/// Salsa's `debug_name` returns just the function name (e.g.
/// `"file_semantic_index"`) with no crate qualification, so we can't rely
/// on `contains("baml_compiler2_hir")` etc. Instead we map by *known
/// query function name* — the list is small (~40 unique queries at last
/// count) so a hardcoded mapping is manageable and gives an accurate
/// phase breakdown.
///
/// If you add a new tracked query to the compiler and want it grouped,
/// add its name here.
fn phase_for_query(name: &str) -> &'static str {
    // Strip our own disambiguation suffix `" [IngredientIndex(N)]"` if
    // present so both collision variants map to the same phase.
    let base = name.split(" [").next().unwrap_or(name);
    match base {
        // Lexer / parser / syntax
        "lex_file" => "lexer",
        "parse_result" | "parse_errors" | "syntax_tree" => "parser",

        // HIR (baml_compiler2_hir)
        "file_semantic_index"
        | "file_ast"
        | "file_item_tree"
        | "file_package"
        | "namespace_items"
        | "package_items"
        | "function_signature"
        | "function_body"
        | "function_parameter_defaults"
        | "function_in_scope_generic_param_bounds"
        | "class_generic_param_bounds"
        | "compiler2_all_files" => "hir",

        // PPIR (baml_compiler2_ppir)
        "ppir_expansion_items" | "file_semantic_index_expanded" => "ppir",

        // Type provider (baml_compiler2_hir_ty)
        "infer_function_body"
        | "infer_let_body"
        | "infer_parameter_defaults"
        | "function_signature_ty"
        | "resolve_class_fields"
        | "resolve_type_alias"
        | "resolve_name_at"
        | "callable_throws"
        | "package_resolved_aliases"
        | "package_impl_locs"
        | "impl_data"
        | "impl_data_source_map"
        | "validate_impl_signatures"
        | "package_coherence_diagnostics"
        | "package_resolution_context" => "ty",

        // MIR (baml_compiler2_mir)
        "lower_function"
        | "lower_let_body"
        | "package_lowering_data"
        | "class_type_tags_for_project" => "mir",

        // Emit (baml_compiler2_emit)
        "generate_project_bytecode" => "emit",

        _ => "other",
    }
}

fn print_human(
    report: &RunReport,
    all_runs: &[RunReport],
    bytes: usize,
    lines: usize,
    files: usize,
    top_n: usize,
) {
    println!();
    println!("=== BAML compile profile ===");
    println!(
        "workload: {} files, {} lines, {} bytes",
        files, lines, bytes
    );
    // Explicit cache-mode line — makes it impossible to misread the
    // report as a warm/incremental measurement when it isn't.
    let cold_count = all_runs.iter().filter(|r| r.mode.is_cold()).count();
    let warm_count = all_runs.iter().filter(|r| !r.mode.is_cold()).count();
    println!(
        "cache mode: cold (fresh Salsa db per run){}",
        if warm_count > 0 {
            format!(
                " + {} warm re-run(s) per cold run, on the same db",
                warm_count / cold_count.max(1)
            )
        } else {
            String::new()
        },
    );

    // Wall-clock breakdown
    println!();
    println!(
        "--- Wall clock (representative COLD run, {} cold + {} warm invocation(s) total) ---",
        cold_count, warm_count,
    );
    println!(
        "  {:<20} {:>10.3} s",
        "db build (inputs)",
        report.db_build.as_secs_f64()
    );
    println!("  {:<20} {:>10.3} s", "check", report.check.as_secs_f64());
    if report.emit_attempted {
        println!(
            "  {:<20} {:>10.3} s",
            "emit (bytecode)",
            report.emit.as_secs_f64()
        );
    } else {
        println!(
            "  {:<20} {:>10}   (skipped: {})",
            "emit (bytecode)",
            "-",
            if report.error_count > 0 {
                "check reported errors"
            } else {
                "--check-only"
            }
        );
    }
    println!("  {:<20} {:>10.3} s", "TOTAL", report.total.as_secs_f64());

    if report.error_count + report.warning_count > 0 {
        println!(
            "  diagnostics: {} error(s), {} warning(s)",
            report.error_count, report.warning_count
        );
    }

    // Cold-only variance table (only shown when there are ≥2 cold runs).
    let cold_runs: Vec<&RunReport> = all_runs.iter().filter(|r| r.mode.is_cold()).collect();
    if cold_runs.len() > 1 {
        let min = cold_runs
            .iter()
            .map(|r| r.total.as_secs_f64())
            .fold(f64::INFINITY, f64::min);
        let max = cold_runs
            .iter()
            .map(|r| r.total.as_secs_f64())
            .fold(f64::NEG_INFINITY, f64::max);
        let mean =
            cold_runs.iter().map(|r| r.total.as_secs_f64()).sum::<f64>() / cold_runs.len() as f64;
        println!();
        println!("--- Cold-run variance across {} runs ---", cold_runs.len());
        println!(
            "  total: min {:.3}s  median {:.3}s  mean {:.3}s  max {:.3}s",
            min,
            report.total.as_secs_f64(),
            mean,
            max
        );
    }

    // Cold vs warm comparison — the point of `--warm-runs`. If warm
    // totals are near zero, Salsa's cache is doing what it should. If
    // warm totals still spend real time in `check` or `emit`, we have a
    // caching gap: a tracked query is being re-executed even though its
    // inputs didn't change.
    if warm_count > 0 {
        println!();
        println!("--- Cold vs warm (per-invocation, same-db reruns) ---");
        println!(
            "  {:<10} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "mode", "total_s", "check_s", "emit_s", "exec", "hits"
        );
        for r in all_runs {
            println!(
                "  {:<10} {:>10.3} {:>10.3} {:>10.3} {:>10} {:>10}",
                r.mode.label(),
                r.total.as_secs_f64(),
                r.check.as_secs_f64(),
                r.emit.as_secs_f64(),
                total_executions(&r.queries),
                total_cache_hits(&r.queries),
            );
        }
        // Cross-check: three distinct failure modes to call out.
        //
        //  1) Warm runs *re-execute* queries: caching gap inside Salsa
        //     (input keys aren't stable, or the query isn't tracked).
        //  2) Warm runs execute nothing but still spend real time:
        //     work is happening OUTSIDE any Salsa query — the wrapper
        //     is doing materialization / walking / cloning on every
        //     call. Salsa's cache can't help this; the fix is to
        //     memoize (or move) the work.
        //  3) Warm total ≈ 0: everything is cached, no lead.
        let warm_reports: Vec<&RunReport> = all_runs.iter().filter(|r| !r.mode.is_cold()).collect();
        let worst_warm_exec = warm_reports
            .iter()
            .map(|r| total_executions(&r.queries))
            .max()
            .unwrap_or(0);
        let worst_warm_check = warm_reports
            .iter()
            .map(|r| r.check.as_secs_f64())
            .fold(0.0_f64, f64::max);
        let worst_warm_emit = warm_reports
            .iter()
            .map(|r| r.emit.as_secs_f64())
            .fold(0.0_f64, f64::max);
        println!();
        if worst_warm_exec > 0 {
            println!(
                "  ⚠ warm runs re-executed up to {worst_warm_exec} queries. Any query still \
                 firing on a warm run is a caching gap — its inputs are stable but it isn't \
                 being memoized (or its key is unstable across calls)."
            );
        }
        // Threshold: 100ms of warm wall-time in a phase with 0 exec
        // is enough to be worth investigating. Below that it's likely
        // just Salsa's revision-check bookkeeping.
        if worst_warm_exec == 0 && (worst_warm_check > 0.1 || worst_warm_emit > 0.1) {
            println!(
                "  ⚠ warm runs executed 0 queries but still spent significant wall time \
                 (check={:.3}s, emit={:.3}s). That work lives OUTSIDE any Salsa tracked \
                 query, so Salsa's cache cannot help it. Look at what `db.check()` and \
                 `db.get_bytecode()` do between their tracked-query calls — the wrapper is \
                 doing per-invocation work (materialization, walking, cloning, printing).",
                worst_warm_check, worst_warm_emit
            );
        }
        if worst_warm_exec == 0 && worst_warm_check <= 0.1 && worst_warm_emit <= 0.1 {
            println!(
                "  ✓ warm runs are fully cached (0 executions, ≤100ms of wall time in every \
                 phase). Salsa is doing its job on this workload."
            );
        }
    }

    // Aggregate query stats
    let resolved = report.queries.as_slice();
    let total_exec: u64 = resolved.iter().map(|r| r.executed).sum();
    let total_hits: u64 = resolved.iter().map(|r| r.cache_hits).sum();
    let total_blocks: u64 = resolved.iter().map(|r| r.blocked).sum();
    let total_events = total_exec + total_hits;
    let hit_rate = if total_events == 0 {
        0.0
    } else {
        total_hits as f64 / total_events as f64 * 100.0
    };

    println!();
    println!("--- Query events (aggregate) ---");
    println!("  executions:  {:>12}", total_exec);
    println!(
        "  cache hits:  {:>12}   ({:.2}% of resolves)",
        total_hits, hit_rate
    );
    println!("  blocked on:  {:>12}", total_blocks);
    println!("  cycle iters: {:>12}", report.cycle_iterations);
    println!("  unique queries: {:>9}", resolved.len());

    // Per-phase aggregate
    let mut phase_totals: HashMap<&'static str, (u64, u64)> = HashMap::new();
    for row in resolved {
        let phase = phase_for_query(&row.name);
        let entry = phase_totals.entry(phase).or_default();
        entry.0 += row.executed;
        entry.1 += row.cache_hits;
    }
    let mut phase_rows: Vec<_> = phase_totals.into_iter().collect();
    phase_rows.sort_by_key(|row| std::cmp::Reverse(row.1.0));
    println!();
    println!("--- Query events by phase ---");
    println!(
        "  {:<12} {:>12} {:>12} {:>10}",
        "phase", "executions", "cache hits", "hit %"
    );
    for (phase, (exec, hits)) in &phase_rows {
        let total = exec + hits;
        let pct = if total == 0 {
            0.0
        } else {
            *hits as f64 / total as f64 * 100.0
        };
        println!("  {:<12} {:>12} {:>12} {:>9.2}%", phase, exec, hits, pct);
    }

    // Top-N by executions
    println!();
    println!(
        "--- Top {} queries by executions (uncached work) ---",
        top_n
    );
    println!(
        "  {:<8} {:<8} {:<8} {:<8}  query",
        "exec", "hits", "blocked", "hit%"
    );
    for row in resolved.iter().take(top_n) {
        let total = row.executed + row.cache_hits;
        let pct = if total == 0 {
            0.0
        } else {
            row.cache_hits as f64 / total as f64 * 100.0
        };
        println!(
            "  {:<8} {:<8} {:<8} {:<7.1}% {}",
            row.executed, row.cache_hits, row.blocked, pct, row.name
        );
    }

    // Top-N by (exec + hits): most-touched queries. High counts here mean
    // the query is on the hot path; combined with a low hit% these are
    // candidates for redundant work.
    let mut by_touches: Vec<&QueryRow> = resolved.iter().collect();
    by_touches.sort_by(|a, b| {
        (b.executed + b.cache_hits)
            .cmp(&(a.executed + a.cache_hits))
            .then_with(|| a.name.cmp(&b.name))
    });
    println!();
    println!("--- Top {} queries by total calls (exec + hits) ---", top_n);
    println!(
        "  {:<10} {:<8} {:<8} {:<8}  query",
        "calls", "exec", "hits", "hit%"
    );
    for row in by_touches.iter().take(top_n) {
        let calls = row.executed + row.cache_hits;
        let pct = if calls == 0 {
            0.0
        } else {
            row.cache_hits as f64 / calls as f64 * 100.0
        };
        println!(
            "  {:<10} {:<8} {:<8} {:<7.1}% {}",
            calls, row.executed, row.cache_hits, pct, row.name
        );
    }

    // Queries where execution >> cache_hits: suspicious.
    // Filter out queries with tiny counts (< 100) to keep the noise down —
    // for those it doesn't matter whether they're cached.
    let mut suspect: Vec<&QueryRow> = resolved
        .iter()
        .filter(|r| r.executed >= 100 && r.executed >= r.cache_hits * 2)
        .collect();
    suspect.sort_by_key(|row| std::cmp::Reverse(row.executed));
    println!();
    println!("--- Suspect: high exec, low cache hit (exec ≥ 100, exec ≥ 2× hits) ---");
    if suspect.is_empty() {
        println!("  (none)");
    } else {
        println!("  {:<8} {:<8} {:<8}  query", "exec", "hits", "hit%");
        for row in suspect.iter().take(top_n) {
            let total = row.executed + row.cache_hits;
            let pct = if total == 0 {
                0.0
            } else {
                row.cache_hits as f64 / total as f64 * 100.0
            };
            println!(
                "  {:<8} {:<8} {:<7.1}% {}",
                row.executed, row.cache_hits, pct, row.name
            );
        }
    }

    println!();
    println!("Tips:");
    println!("  * For a CPU flamegraph, wrap this binary with a sampler:");
    println!("      samply record ./target/release/tools_compile_profile <project>");
    println!("  * The 'suspect' table above is a first place to look for repeated");
    println!("    work — a query that fires many times but is rarely a cache hit");
    println!("    is either recomputing per-call, or keyed too finely.");
    println!();
}

fn print_json(
    report: &RunReport,
    all_runs: &[RunReport],
    bytes: usize,
    lines: usize,
    files: usize,
) {
    #[derive(serde::Serialize)]
    struct Out<'a> {
        files: usize,
        lines: usize,
        bytes: usize,
        representative: RunOut<'a>,
        all_runs: Vec<RunOut<'a>>,
    }
    #[derive(serde::Serialize)]
    struct RunOut<'a> {
        /// "cold" or "warm#N" — see `RunMode`.
        mode: String,
        total_seconds: f64,
        db_build_seconds: f64,
        check_seconds: f64,
        emit_seconds: f64,
        emit_attempted: bool,
        error_count: usize,
        warning_count: usize,
        total_executions: u64,
        total_cache_hits: u64,
        cycle_iterations: u64,
        queries: &'a [QueryRow],
    }
    fn to_out(r: &RunReport) -> RunOut<'_> {
        RunOut {
            mode: r.mode.label(),
            total_seconds: r.total.as_secs_f64(),
            db_build_seconds: r.db_build.as_secs_f64(),
            check_seconds: r.check.as_secs_f64(),
            emit_seconds: r.emit.as_secs_f64(),
            emit_attempted: r.emit_attempted,
            error_count: r.error_count,
            warning_count: r.warning_count,
            total_executions: total_executions(&r.queries),
            total_cache_hits: total_cache_hits(&r.queries),
            cycle_iterations: r.cycle_iterations,
            queries: r.queries.as_slice(),
        }
    }
    let out = Out {
        files,
        lines,
        bytes,
        representative: to_out(report),
        all_runs: all_runs.iter().map(to_out).collect(),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn print_summary_line(report: &RunReport, files: usize, lines: usize, bytes: usize) {
    println!(
        "SUMMARY files={} lines={} bytes={} total_s={:.3} check_s={:.3} emit_s={:.3} exec={} hits={} unique={}",
        files,
        lines,
        bytes,
        report.total.as_secs_f64(),
        report.check.as_secs_f64(),
        report.emit.as_secs_f64(),
        total_executions(&report.queries),
        total_cache_hits(&report.queries),
        report.queries.len(),
    );
}

#[cfg(test)]
mod tests {
    use super::phase_for_query;

    #[test]
    fn phase_for_query_maps_known_queries() {
        assert_eq!(phase_for_query("lex_file"), "lexer");
        assert_eq!(phase_for_query("parse_result"), "parser");
        assert_eq!(phase_for_query("syntax_tree"), "parser");
        assert_eq!(phase_for_query("file_semantic_index"), "hir");
        assert_eq!(phase_for_query("function_body"), "hir");
        assert_eq!(phase_for_query("ppir_expansion_items"), "ppir");
        assert_eq!(phase_for_query("infer_function_body"), "ty");
        assert_eq!(phase_for_query("lower_function"), "mir");
        assert_eq!(phase_for_query("generate_project_bytecode"), "emit");
    }

    #[test]
    fn phase_for_query_falls_back_to_other() {
        assert_eq!(phase_for_query("some_unregistered_query"), "other");
        assert_eq!(phase_for_query(""), "other");
    }

    #[test]
    fn phase_for_query_strips_collision_suffix() {
        // Collision-disambiguated names (see `resolve_query_names`) still map to
        // the same phase as their un-suffixed base.
        assert_eq!(
            phase_for_query("file_semantic_index [IngredientIndex(50)]"),
            "hir"
        );
        assert_eq!(
            phase_for_query("infer_function_body [IngredientIndex(7)]"),
            "ty"
        );
        assert_eq!(
            phase_for_query("some_unregistered_query [IngredientIndex(1)]"),
            "other"
        );
    }
}
