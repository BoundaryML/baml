//! Single-compile snapshot pass over the whole `baml_src/` corpus.
//!
//! Every project that compiles cleanly lives in `baml_src/`: the runtime test
//! namespaces (executed by `baml test` via `tests/baml_src.rs`) alongside the
//! compile-only compiler-phase fixtures under `ns_fixtures/` (excluded from
//! execution by the `offline` profile in `baml_src/baml.toml`). This module
//! builds ONE `ProjectDatabase` over the corpus and derives every snapshot
//! from that shared compile:
//!
//! - per-namespace diagnostics snapshots plus the corpus-wide zero-error
//!   invariant,
//! - per-fixture-namespace PPIR and MIR snapshots,
//! - per-namespace bytecode snapshots for the whole corpus,
//! - stdlib (`baml`/`testing`/`assert`/`ai`) PPIR/MIR/bytecode snapshots.
//!
//! The snapshot tree mirrors the corpus source tree: a namespace's snapshots
//! live in `snapshots/baml_src/<same ns_ path>/`, named for their phase
//! (`ppir.snap`, `mir.snap`, `bytecode.snap`), with per-file formatter output
//! as `<file stem>.fmt.snap` beside them. Every namespace occupies exactly one
//! directory, so grouping per namespace and mirroring the directory tree are
//! the same partition.
//!
//! The predecessor harness (`projects/compiles/` + build.rs-generated tests)
//! re-ran the compiler pipeline per project *and* per phase, each in its own
//! process with no shared Salsa cache — ~23 CPU-minutes, almost all of it
//! redundant stdlib recompilation. This pass pays for the pipeline once and
//! reads every phase out of the same database; emit shares the `OptLevel::Two`
//! MIR memos with the MIR snapshots.

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::{Component, Path, PathBuf},
};

use baml_base::SourceFile;
use baml_compiler2_mir::{OptLevel, lower_function, pretty::display_function};
use baml_compiler2_ppir::item_data::{file_functions, function_source_map};
use baml_db::ProjectDatabase;
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, FunctionOrigin};

use crate::engine::TestDbExt;

const SNAPSHOT_BASE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/baml_src");

/// The stdlib packages with dedicated phase snapshots. The remaining builtin
/// packages (provider clients etc.) are covered indirectly through the corpus
/// namespaces that exercise them.
const SNAPSHOT_STDLIB_PACKAGES: &[&str] = &["baml", "testing", "assert", "ai", "reflect"];

fn baml_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src")
}

/// Read every `*.baml` file under `baml_src/`, returning `(relative_path,
/// content)` pairs sorted by path so the compiled project is deterministic.
fn read_corpus_files() -> Vec<(String, String)> {
    let root = baml_src_dir();
    let mut files = Vec::new();
    collect_baml_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn collect_baml_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir baml_src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            // Skip hidden dirs (e.g. a stray `.baml/cache` a CLI run may have
            // left behind); the corpus is only the checked-in `.baml` sources.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
            {
                continue;
            }
            collect_baml_files(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .expect("strip baml_src prefix")
                .to_string_lossy()
                .replace('\\', "/");
            // Normalize line endings so snapshots match across platforms.
            let content = std::fs::read_to_string(&path)
                .expect("read .baml file")
                .replace("\r\n", "\n");
            out.push((rel, content));
        }
    }
}

/// Strip the `ns_` prefix from a directory segment if it names a valid namespace
/// (BAML identifier: starts with a letter or `_`, rest alphanumeric or `_`),
/// matching the compiler's `file_package` rule. Returns `None` otherwise.
fn extract_ns_name(component: &str) -> Option<&str> {
    let ns = component.strip_prefix("ns_")?;
    let mut chars = ns.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(ns)
}

/// Namespace key for a source file: the dotted chain of `ns_`-prefixed directory
/// segments, e.g. `ns_assignments/assignments.baml` -> `assignments` and
/// `ns_a/b/ns_c/x.baml` -> `a.c`. Functions with no source file (synthesized,
/// e.g. the global `$init_test`) or no namespace -> `_root`.
fn namespace_key(source_file: &str) -> String {
    if source_file.is_empty() {
        return "_root".to_string();
    }
    let parts: Vec<&str> = Path::new(source_file)
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str().and_then(extract_ns_name),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if parts.is_empty() {
        "_root".to_string()
    } else {
        parts.join(".")
    }
}

/// Directory of a corpus-relative source path — the namespace's own directory.
/// A namespace occupies exactly one directory in this corpus, so this doubles
/// as the namespace's identity for grouping. Empty for the corpus root, which
/// also absorbs synthesized functions (no source file), matching where the
/// root namespace's own sources live.
fn source_dir(source_file: &str) -> String {
    Path::new(source_file)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// Emit one snapshot under `snapshots/baml_src[/<dir>]/<name>.snap`.
///
/// `dir` mirrors the corpus source tree, so a namespace's snapshots sit in the
/// snapshot tree exactly where its sources sit in `baml_src/`.
fn snap(dir: &str, name: &str, content: &str) {
    let path = if dir.is_empty() {
        SNAPSHOT_BASE.to_string()
    } else {
        format!("{SNAPSHOT_BASE}/{dir}")
    };
    insta::with_settings!({
        snapshot_path => path,
        omit_expression => true,
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(name, content);
    });
}

/// Is this function name in a stdlib package? The package list is derived from
/// `baml_builtins2::ALL`, so adding a builtin package never floods the user
/// namespaces' snapshots.
fn is_stdlib_function(name: &str) -> bool {
    baml_builtins2::stdlib_package_names().iter().any(|pkg| {
        let pkg: &str = pkg;
        name.len() > pkg.len() && name.as_bytes()[pkg.len()] == b'.' && name.starts_with(pkg)
    })
}

/// MIR for every function of `file`, in source order (by declaration span) —
/// an intrinsic, salsa-enumeration-independent key, so the snapshot never
/// churns on a firewall/tie-break change the way a name sort would.
fn render_file_mir(db: &ProjectDatabase, file: SourceFile, out: &mut String) {
    let mut functions = file_functions(db, file).to_vec();
    functions.sort_by_key(|loc| function_source_map(db, *loc).span.start());
    for func_loc in functions {
        let mir = lower_function(db, func_loc, OptLevel::Two);
        writeln!(out, "{}", display_function(&mir)).unwrap();
    }
}

#[test]
fn corpus_snapshots() {
    use baml_compiler_diagnostics::{Diagnostic, DiagnosticPhase, RenderConfig, render_diagnostic};

    let files = read_corpus_files();
    assert!(!files.is_empty(), "no .baml files found in baml_src/");

    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    let mut source_files: Vec<(String, SourceFile)> = Vec::with_capacity(files.len());
    for (rel, content) in &files {
        let sf = db.file(rel, content);
        source_files.push((rel.clone(), sf));
    }

    // ---- Diagnostics: distributed per namespace + the zero-error invariant ----
    let diagnostics = baml_db::collect_compiler2_diagnostics(&db);

    let all_files = baml_compiler2_hir::compiler2_all_files(&db);
    let mut sources: HashMap<baml_db::FileId, String> = HashMap::new();
    let mut file_paths: HashMap<baml_db::FileId, PathBuf> = HashMap::new();
    for source_file in &all_files {
        let file_id = source_file.file_id(&db);
        sources.insert(file_id, source_file.text(&db).to_string());
        file_paths.insert(file_id, source_file.path(&db));
    }

    // Diagnostic spans must tightly cover the offending construct, never the
    // leading/trailing whitespace around it.
    crate::utils::assert_diagnostic_spans_exclude_trivia("baml_src", &diagnostics, &sources);

    let config = RenderConfig::test();
    let phase_name = |phase: DiagnosticPhase| match phase {
        DiagnosticPhase::Parse => "parse",
        DiagnosticPhase::Hir => "hir",
        DiagnosticPhase::Validation => "validation",
        DiagnosticPhase::Type => "type",
    };

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == baml_compiler_diagnostics::Severity::Error)
        .collect();
    if !errors.is_empty() {
        let mut rendered = String::new();
        for diag in &errors {
            let line = render_diagnostic(diag, &sources, &file_paths, &config);
            writeln!(rendered, "  [{}] {}", phase_name(diag.phase), line).unwrap();
        }
        panic!(
            "the baml_src corpus must compile with zero error diagnostics, got {}:\n{rendered}",
            errors.len(),
        );
    }

    // Route each diagnostic to its namespace's own directory, so a warning
    // sits beside the phase snapshots of the code that produced it. A
    // namespace with no diagnostics simply has no `diagnostics.snap`.
    //
    // The two non-corpus cases get their own visible destination rather than
    // being dropped: a diagnostic on a builtin file lands in `stdlib/`, and one
    // with no primary span (hence no file) in `_unlocated.diagnostics.snap`.
    // Neither occurs today, so neither file exists — if one ever appears, it
    // appears as a new snapshot instead of vanishing from the suite.
    let corpus_paths: HashMap<baml_db::FileId, String> = source_files
        .iter()
        .map(|(rel, sf)| (sf.file_id(&db), rel.clone()))
        .collect();
    let destination = |diag: &Diagnostic| -> (String, String) {
        match diag.file_id() {
            Some(file_id) => match corpus_paths.get(&file_id) {
                Some(rel) => (source_dir(rel), "diagnostics".to_string()),
                None => ("stdlib".to_string(), "diagnostics".to_string()),
            },
            None => (String::new(), "_unlocated.diagnostics".to_string()),
        }
    };

    let mut by_dir: BTreeMap<(String, String), Vec<&Diagnostic>> = BTreeMap::new();
    for diag in &diagnostics {
        by_dir.entry(destination(diag)).or_default().push(diag);
    }

    for ((dir, name), mut diags) in by_dir {
        // Order by (file, offset) — intrinsic source keys, so the snapshot
        // never churns on the order the collector happened to produce.
        diags.sort_by_key(|diag| {
            let path = diag
                .file_id()
                .and_then(|file_id| corpus_paths.get(&file_id).cloned())
                .unwrap_or_default();
            (path, diag.primary_span().map(|span| span.range.start()))
        });

        let mut output = String::new();
        writeln!(output, "=== COMPILER2 DIAGNOSTICS ===").unwrap();
        for diag in diags {
            let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
            writeln!(output, "  [{}] {}", phase_name(diag.phase), rendered).unwrap();
        }
        snap(&dir, &name, &output);
    }

    // ---- Fixture namespaces: PPIR + MIR ----
    let mut fixture_dirs: BTreeMap<String, Vec<SourceFile>> = BTreeMap::new();
    for (rel, sf) in &source_files {
        if rel.starts_with("ns_fixtures/") {
            fixture_dirs.entry(source_dir(rel)).or_default().push(*sf);
        }
    }
    assert!(
        !fixture_dirs.is_empty(),
        "no fixture namespaces found under baml_src/ns_fixtures/"
    );

    for (dir, ns_files) in &fixture_dirs {
        let mut out = String::new();
        writeln!(out, "=== PPIR ===").unwrap();
        for sf in ns_files {
            out.push_str(&crate::compiler2_tir::support::render_ppir(&db, *sf));
        }
        snap(dir, "ppir", &out);

        let mut out = String::new();
        writeln!(out, "=== MIR2 ===").unwrap();
        for sf in ns_files {
            render_file_mir(&db, *sf, &mut out);
        }
        snap(dir, "mir", &out);
    }

    // ---- Stdlib phase snapshots ----
    for pkg in SNAPSHOT_STDLIB_PACKAGES {
        use baml_compiler2_hir::file_package::file_package;
        let mut pkg_files: Vec<_> = all_files
            .iter()
            .copied()
            .filter(|f| file_package(&db, *f).package.as_str() == *pkg)
            .collect();
        pkg_files.sort_by_key(|f| f.path(&db).to_string_lossy().to_string());

        let mut out = String::new();
        writeln!(out, "=== PPIR (package {pkg}) ===").unwrap();
        for sf in &pkg_files {
            writeln!(out, "\n--- {} ---", sf.path(&db).display()).unwrap();
            out.push_str(&crate::compiler2_tir::support::render_ppir(&db, *sf));
        }
        snap(&format!("stdlib/{pkg}"), "ppir", &out);

        let mut out = String::new();
        writeln!(out, "=== MIR2 (package {pkg}) ===").unwrap();
        for sf in &pkg_files {
            render_file_mir(&db, *sf, &mut out);
        }
        snap(&format!("stdlib/{pkg}"), "mir", &out);
    }

    // ---- Bytecode: one emit, snapshotted per namespace ----
    // `OptLevel::Two` matches what the deleted per-project codegen tests used
    // (`generate_project_bytecode` defaults), and O2 lets emit reuse the MIR
    // memos the snapshots above populated.
    let program = baml_compiler2_emit::generate_project_bytecode(&db)
        .expect("bytecode emit should succeed for an error-free corpus");

    // Emit mints tag-only type heads (the pointer half exists only once a heap
    // does), so a signature rendered off the raw program would show
    // `<unresolved type #tag>`. Display heap-resident functions instead: the
    // bound pool resolves every head, matching what the runtime shows.
    let heap = crate::engine::bound_pool(&program);

    // Group user (non-stdlib, non-auto-derived) functions by their namespace
    // directory, so each namespace's bytecode lands beside its own sources.
    let mut by_dir: BTreeMap<String, Vec<(String, &Function)>> = BTreeMap::new();
    for (name, idx) in &program.function_indices {
        // Stdlib functions are not the user namespaces' subject; they get the
        // dedicated per-package snapshots below.
        if is_stdlib_function(name) || name.starts_with("env.") {
            continue;
        }
        let Some(func) = crate::engine::bound_function(&heap, *idx) else {
            continue;
        };
        if func.origin == FunctionOrigin::AutoDerive {
            continue;
        }
        // The `llm_*` provider-suite namespaces are large wire-shape/behavior
        // suites whose guarantees live in their own `test` blocks; their
        // bytecode dumps flooded these snapshots (thousands of lines each)
        // without adding signal. Codegen stability is still covered by the
        // remaining namespaces (including the `ns_fixtures/` ones).
        if namespace_key(&func.source_file).starts_with("llm_") {
            continue;
        }
        // Strip the leading "user." package prefix for display.
        let display_name = name.strip_prefix("user.").unwrap_or(name).to_owned();
        by_dir
            .entry(source_dir(&func.source_file))
            .or_default()
            .push((display_name, func));
    }

    for (dir, mut funcs) in by_dir {
        funcs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let output = display_program(&funcs, BytecodeFormat::Textual);
        snap(&dir, "bytecode", &output);
    }

    // Stdlib bytecode, one snapshot per package.
    for pkg in SNAPSHOT_STDLIB_PACKAGES {
        let prefix = format!("{pkg}.");
        let mut func_names: Vec<_> = program
            .function_indices
            .keys()
            .filter(|name| name.starts_with(&prefix))
            .collect();
        func_names.sort();

        let functions: Vec<(String, &Function)> = func_names
            .iter()
            .map(|name| {
                let idx = *program.function_indices.get(*name).unwrap();
                let func = crate::engine::bound_function(&heap, idx).unwrap_or_else(|| {
                    panic!("function_indices entry '{name}' (idx={idx}) is not a Function")
                });
                ((*name).clone(), func)
            })
            .collect();

        let output = display_program(&functions, BytecodeFormat::Textual);
        snap(&format!("stdlib/{pkg}"), "bytecode", &output);
    }
}

/// Corpus files the formatter is known to reject today. Every entry MUST keep
/// failing: a file that starts formatting makes the test fail until its entry
/// is deleted, so this list cannot rot into a silent skip.
///
/// All of these are formatter strong-AST gaps for constructs the compiler
/// accepts and compiles fine:
// BUG: the formatter's strong-AST builder rejects array rest-binding patterns
// (`if let [let a, ..let r] = xs`): "An element ... was a node when it should
// have been a token".
// BUG: the formatter's strong-AST builder rejects BIGINT_LITERAL in match-arm
// literal positions ("Expected ... INTEGER_LITERAL, or FLOAT_LITERAL, but
// found BIGINT_LITERAL").
// BUG: the formatter's strong-AST builder rejects the `~` bitwise-not operator
// ("Expected token/node unary operator, but found TILDE").
const KNOWN_FORMATTER_REJECTS: &[&str] = &[
    "ns_array_rest_binding/array_rest_binding.baml",
    "ns_bigints/bigints.baml",
    "ns_literal_pattern_membership/literal_pattern_membership.baml",
    "ns_operators/operators.baml",
    "ns_truthiness/truthiness.baml",
];

/// Formatter coverage for the corpus: fixture files get an output snapshot;
/// every corpus file must format successfully and idempotently.
#[test]
fn corpus_formatter() {
    let files = read_corpus_files();
    let options = baml_fmt::FormatOptions::default();

    // Collect every violation so one run reports the full set.
    let mut violations: Vec<String> = Vec::new();

    for (rel, content) in &files {
        let known_reject = KNOWN_FORMATTER_REJECTS.contains(&rel.as_str());
        let first = match baml_fmt::format(content, &options) {
            Ok(_) if known_reject => {
                violations.push(format!(
                    "formatter unexpectedly succeeded on {rel}; \
                     remove it from KNOWN_FORMATTER_REJECTS"
                ));
                continue;
            }
            Ok(formatted) => formatted,
            Err(_) if known_reject => continue,
            Err(e) => {
                let rendered = match e {
                    baml_fmt::FormatterError::ParseErrors(e) => {
                        format!("=== PARSER ERROR ===\n{e:?}")
                    }
                    baml_fmt::FormatterError::StrongAstError(e) => {
                        let e = e.print_with_file_context(rel, content);
                        format!("=== STRONG AST ERROR ===\n{e}")
                    }
                };
                violations.push(format!("formatter rejected corpus file {rel}:\n{rendered}"));
                continue;
            }
        };

        if rel.starts_with("ns_fixtures/") {
            let stem = Path::new(rel)
                .file_stem()
                .expect("corpus file has a stem")
                .to_string_lossy();
            snap(&source_dir(rel), &format!("{stem}.fmt"), &first);
        }

        // Format a second time — the output must be identical (idempotency).
        match baml_fmt::format(&first, &options) {
            Ok(second) if first != second => violations.push(format!(
                "formatter is not idempotent for {rel}: second pass differs"
            )),
            Ok(_) => {}
            Err(e) => violations.push(format!(
                "formatter succeeded on corpus file {rel} but failed on its own output:\n{e}"
            )),
        }
    }

    assert!(
        violations.is_empty(),
        "{} formatter violation(s) across the corpus:\n\n{}",
        violations.len(),
        violations.join("\n\n"),
    );
}
