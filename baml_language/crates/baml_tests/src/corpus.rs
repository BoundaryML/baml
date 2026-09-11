//! Single-compile snapshot pass over the whole `baml_src/` corpus.
//!
//! Every project that compiles cleanly lives in `baml_src/`: the runtime test
//! namespaces (executed via `baml_cli/tests/baml_corpus.rs`) alongside the
//! compile-only compiler-phase fixtures under `ns_fixtures/` (excluded from
//! execution by the `offline` profile in `baml_src/baml.toml`). This module
//! builds ONE `ProjectDatabase` over the corpus and derives every snapshot
//! from that shared compile:
//!
//! - per-namespace diagnostics snapshots plus the corpus-wide zero-error
//!   invariant,
//! - opt-in representative PPIR, MIR and bytecode snapshots,
//! - opt-in formatter goldens, plus formatting/idempotency of every file.
//!
//! `corpus_snapshot_policy.rs` documents each selected example. No blanket
//! stdlib dumps: runtime, prefix-equivalence and link-oracle tests cover it.
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
    path::{Path, PathBuf},
};

use baml_base::SourceFile;
use baml_compiler2_mir::{OptLevel, lower_function, pretty::display_function};
use baml_compiler2_ppir::item_data::{file_functions, function_source_map};
use baml_db::ProjectDatabase;
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::Function;

use crate::engine::TestDbExt;

const SNAPSHOT_BASE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/baml_src");

#[path = "corpus_snapshot_policy.rs"]
mod snapshot_policy;

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

/// Validate selectors before doing compiler work: a renamed or duplicated
/// example must fail loudly instead of silently losing golden coverage.
fn validate_snapshot_policy(files: &[(String, String)]) {
    for (phase, examples) in [
        ("ppir", snapshot_policy::PPIR),
        ("mir", snapshot_policy::MIR),
        ("bytecode", snapshot_policy::BYTECODE),
        ("formatter", snapshot_policy::FORMATTER),
    ] {
        let mut destinations = std::collections::BTreeSet::new();
        for example in examples {
            assert!(
                !example.reason.trim().is_empty(),
                "{phase}: missing coverage rationale"
            );
            assert!(
                files.iter().any(|(path, _)| path == example.path),
                "{phase}: selected source is missing: {}",
                example.path
            );
            let key = if phase == "formatter" {
                example.path.to_owned()
            } else {
                source_dir(example.path)
            };
            assert!(
                destinations.insert(key),
                "{phase}: duplicate snapshot destination for {}",
                example.path
            );
            if phase == "mir" || phase == "bytecode" {
                assert!(
                    !example.functions.is_empty(),
                    "{phase}: select specific functions"
                );
                let unique: std::collections::BTreeSet<_> = example.functions.iter().collect();
                assert_eq!(
                    unique.len(),
                    example.functions.len(),
                    "{phase}: duplicate function selector"
                );
            }
        }
    }
}

/// MIR for selected functions in source order. Everything else is still
/// lowered by full-corpus bytecode emission, but does not get a textual dump.
fn render_selected_mir(db: &ProjectDatabase, file: SourceFile, names: &[&str], out: &mut String) {
    let mut functions = file_functions(db, file).to_vec();
    functions.sort_by_key(|loc| function_source_map(db, *loc).span.start());
    let mut found = std::collections::BTreeSet::new();
    for func_loc in functions {
        let name_span = function_source_map(db, func_loc).name_span;
        let name = &file.text(db)[name_span];
        if names.contains(&name) {
            found.insert(name.to_owned());
            let mir = lower_function(db, func_loc, OptLevel::Two);
            writeln!(out, "{}", display_function(mir)).unwrap();
        }
    }
    for name in names {
        assert!(
            found.contains(*name),
            "MIR snapshot function {name} missing from {}",
            file.path(db).display()
        );
    }
}

#[test]
fn snapshot_policy_is_valid() {
    validate_snapshot_policy(&read_corpus_files());
}

/// Catch orphaned goldens even in a narrow nextest run, where Insta's
/// package-wide --unreferenced check would also flag unrelated test suites.
#[test]
fn snapshot_inventory_matches_policy() {
    fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read snapshot directory") {
            let path = entry.expect("snapshot entry").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "snap") {
                files.push(path);
            }
        }
    }
    let root = Path::new(SNAPSHOT_BASE);
    let mut expected = std::collections::BTreeSet::new();
    for (phase, examples) in [
        ("ppir", snapshot_policy::PPIR),
        ("mir", snapshot_policy::MIR),
        ("bytecode", snapshot_policy::BYTECODE),
        ("fmt", snapshot_policy::FORMATTER),
    ] {
        for example in examples {
            let name = if phase == "fmt" {
                format!(
                    "{}.fmt.snap",
                    Path::new(example.path)
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                )
            } else {
                format!("{phase}.snap")
            };
            expected.insert(root.join(source_dir(example.path)).join(name));
        }
    }
    let mut files = Vec::new();
    collect(root, &mut files);
    let actual: std::collections::BTreeSet<_> = files
        .into_iter()
        // Diagnostics remain exhaustive and are generated for nonempty groups.
        .filter(|path| {
            !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("diagnostics.snap")
        })
        .collect();
    assert_eq!(
        actual, expected,
        "missing or orphaned corpus goldens; update the policy and snapshots together"
    );
}

#[test]
fn corpus_snapshots() {
    use baml_compiler_diagnostics::{Diagnostic, DiagnosticPhase, RenderConfig, render_diagnostic};

    let files = read_corpus_files();
    assert!(!files.is_empty(), "no .baml files found in baml_src/");
    validate_snapshot_policy(&files);

    let mut db = ProjectDatabase::new();
    let package = db.workspace(Path::new("."));
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

    // ---- Representative PPIR and MIR only; still use the shared database ----
    let selected_file = |path: &str| {
        source_files
            .iter()
            .find(|(rel, _)| rel == path)
            .unwrap_or_else(|| panic!("selected source missing: {path}"))
            .1
    };
    for example in snapshot_policy::PPIR {
        let out = format!(
            "=== PPIR ===\n{}",
            crate::compiler2_tir::support::render_ppir(&db, selected_file(example.path))
        );
        snap(&source_dir(example.path), "ppir", &out);
    }
    for example in snapshot_policy::MIR {
        let mut out = String::from("=== MIR2 ===\n");
        render_selected_mir(
            &db,
            selected_file(example.path),
            example.functions,
            &mut out,
        );
        snap(&source_dir(example.path), "mir", &out);
    }

    // ---- Bytecode: one emit, snapshotted per namespace ----
    // `OptLevel::Two` matches what the deleted per-project codegen tests used
    // (`generate_project_bytecode` defaults), and O2 lets emit reuse the MIR
    // memos the snapshots above populated.
    let program = baml_compiler2_emit::generate_project_bytecode(&db, package)
        .expect("bytecode emit should succeed for an error-free corpus");

    // Emit mints tag-only type heads (the pointer half exists only once a heap
    // does), so a signature rendered off the raw program would show
    // `<unresolved type #tag>`. Display heap-resident functions instead: the
    // bound pool resolves every head, matching what the runtime shows.
    let heap = crate::engine::bound_pool(&program);

    // Exact emitted names prevent a growing runtime namespace from silently
    // inflating its golden. Missing functions fail instead of emitting nothing.
    let by_name: BTreeMap<_, _> = crate::engine::named_and_interface_body_functions(&program)
        .map(|(name, idx)| (name.as_str(), idx))
        .collect();
    for example in snapshot_policy::BYTECODE {
        let mut functions: Vec<(String, &Function)> = Vec::new();
        for name in example.functions {
            let idx = *by_name
                .get(name)
                .unwrap_or_else(|| panic!("bytecode snapshot function missing: {name}"));
            let func = crate::engine::bound_function(&heap, idx)
                .unwrap_or_else(|| panic!("bytecode snapshot entry is not a function: {name}"));
            assert_eq!(
                source_dir(&func.source_file),
                source_dir(example.path),
                "wrong source for {name}"
            );
            functions.push((name.strip_prefix("user.").unwrap_or(name).to_string(), func));
        }
        functions.sort_by(|(a, _), (b, _)| a.cmp(b));
        snap(
            &source_dir(example.path),
            "bytecode",
            &display_program(&functions, BytecodeFormat::Textual),
        );
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

/// Formatter coverage for the corpus: selected files get an output snapshot;
/// every corpus file must format successfully and idempotently.
#[test]
fn corpus_formatter() {
    let files = read_corpus_files();
    validate_snapshot_policy(&files);
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

        if snapshot_policy::FORMATTER
            .iter()
            .any(|example| example.path == rel)
        {
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
