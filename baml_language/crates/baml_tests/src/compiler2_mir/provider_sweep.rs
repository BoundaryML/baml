//! S16: the pre-cutover differential MIR sweep - rustc's
//! `-Z borrowck=compare` shape, one level down from the S15 type sweep.
//! Lowers EVERY function of the living corpus (`baml_tests/baml_src/`)
//! under BOTH inference providers and diffs the pretty-printed MIR. The
//! snapshot is the ledger: agreements are counted, every differing
//! function is itemized (first differing line pair as the excerpt),
//! every panic is itemized per side. The exit criterion for the S16
//! flip is an empty diff-and-panic section; the burn-down works the
//! itemized entries against the recorded-table gaps (task #59's list)
//! and the ruled type divergences surfacing as template diffs.
//!
//! Top-level lets are NOT diffed yet (no body-level pretty renderer);
//! the header says so - a bounded gap stated, not silently dropped.

use std::fmt::Write;

use baml_compiler2_mir::{InferenceProvider, OptLevel, lower_function_with, pretty::display_function};
use baml_compiler2_ppir::item_data::{file_functions, function_data, function_source_map};

use crate::type_spec::sweep::{baml_src_dir, read_corpus_files};

/// One function's verdict under the dual lowering.
enum Verdict {
    Agree,
    /// Every differing line fits a documented ruling's bucket.
    Ruled { bucket: &'static str },
    Differ { excerpt: String },
    Panic { side: &'static str, message: String },
}

fn lower_pretty(
    db: &baml_project::ProjectDatabase,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    provider: InferenceProvider,
) -> Result<String, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        display_function(&lower_function_with(db, func_loc, OptLevel::Two, provider))
    }))
    .map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>")
            .to_string()
    })
}

/// The MACHINE-CHECKED ruled-divergence taxonomy, the type sweep's
/// `classify_divergence` discipline one level down: a differing function
/// lands in a bucket only when EVERY differing line pair fits the
/// bucket's rule, each bucket names a documented ruling, and anything
/// else stays itemized. Buckets:
///
/// - `throws-precision`: lines identical except the line-FINAL throws
///   component, where TIR's side is `unknown` or an unresolved rigid
///   effect var and hir_ty's is the inferred effect - the S12/S15 ruled
///   family (hir ahead; TIR cannot infer lambda and instantiated
///   effects).
fn classify_diff(tir: &str, hir: &str) -> Option<&'static str> {
    let tir_lines: Vec<&str> = tir.lines().collect();
    let hir_lines: Vec<&str> = hir.lines().collect();
    if tir_lines.len() != hir_lines.len() {
        return None;
    }
    let mut any = false;
    for (t, h) in tir_lines.iter().zip(&hir_lines) {
        if t == h {
            continue;
        }
        any = true;
        if !throws_precision_pair(t, h) {
            return None;
        }
    }
    any.then_some("throws-precision")
}

/// One line pair differing only in a line-final ` throws X` where TIR's
/// `X` is imprecise: `unknown`, or a bare rigid effect var (a single
/// capitalized identifier).
fn throws_precision_pair(tir: &str, hir: &str) -> bool {
    let (Some(t_split), Some(h_split)) = (tir.rfind(" throws "), hir.rfind(" throws ")) else {
        return false;
    };
    let (t_base, t_throws) = tir.split_at(t_split);
    let (h_base, _) = hir.split_at(h_split);
    if t_base != h_base {
        return false;
    }
    let t_throws = t_throws.trim_start_matches(" throws ").trim_end();
    t_throws == "unknown"
        || (t_throws.len() <= 2
            && t_throws
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

/// The first differing line pair, with one line of shared context above.
fn first_diff_excerpt(tir: &str, hir: &str) -> String {
    let tir_lines: Vec<&str> = tir.lines().collect();
    let hir_lines: Vec<&str> = hir.lines().collect();
    let common = tir_lines
        .iter()
        .zip(&hir_lines)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = String::new();
    if common > 0 {
        out.push_str(&format!("    ... {}\n", tir_lines[common - 1].trim_end()));
    }
    match (tir_lines.get(common), hir_lines.get(common)) {
        (Some(t), Some(h)) => {
            out.push_str(&format!("    tir: {}\n", t.trim_end()));
            out.push_str(&format!("    hir: {}\n", h.trim_end()));
        }
        (Some(t), None) => {
            out.push_str(&format!("    tir: {}\n", t.trim_end()));
            out.push_str("    hir: <ends>\n");
        }
        (None, Some(h)) => {
            out.push_str("    tir: <ends>\n");
            out.push_str(&format!("    hir: {}\n", h.trim_end()));
        }
        (None, None) => out.push_str("    <equal lines, trailing whitespace only>\n"),
    }
    out
}

#[test]
fn s16_provider_sweep_baml_src() {
    let root = baml_src_dir();
    let mut files = Vec::new();
    read_corpus_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!files.is_empty(), "no corpus files found");

    let mut db = crate::compiler2_tir::support::make_db();
    let loaded: Vec<(String, baml_base::SourceFile)> = files
        .into_iter()
        .map(|(rel, content)| {
            let file = db.add_file(&rel, &content);
            (rel, file)
        })
        .collect();

    let mut functions_compared = 0usize;
    let mut agreements = 0usize;
    let mut ruled: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut differing: Vec<(String, String)> = Vec::new();
    let mut panics: Vec<String> = Vec::new();

    for (rel, file) in &loaded {
        let mut functions = file_functions(&db, *file).to_vec();
        functions.sort_by_key(|loc| function_source_map(&db, *loc).span.start());
        for func_loc in functions {
            let name = function_data(&db, func_loc).name.clone();
            let key = format!("{rel} :: {name}");
            functions_compared += 1;
            let verdict = match (
                lower_pretty(&db, func_loc, InferenceProvider::Tir),
                lower_pretty(&db, func_loc, InferenceProvider::HirTy),
            ) {
                (Ok(tir), Ok(hir)) if tir == hir => Verdict::Agree,
                (Ok(tir), Ok(hir)) => match classify_diff(&tir, &hir) {
                    Some(bucket) => Verdict::Ruled { bucket },
                    None => Verdict::Differ {
                        excerpt: first_diff_excerpt(&tir, &hir),
                    },
                },
                (Err(message), _) => Verdict::Panic {
                    side: "tir",
                    message,
                },
                (_, Err(message)) => Verdict::Panic {
                    side: "hir_ty",
                    message,
                },
            };
            match verdict {
                Verdict::Agree => agreements += 1,
                Verdict::Ruled { bucket } => *ruled.entry(bucket).or_insert(0usize) += 1,
                Verdict::Differ { excerpt } => differing.push((key, excerpt)),
                Verdict::Panic { side, message } => {
                    panics.push(format!("{key} [{side}] {message}"))
                }
            }
        }
    }

    let mut report = String::new();
    writeln!(report, "== S16 provider sweep ==").unwrap();
    writeln!(report, "functions compared: {functions_compared}").unwrap();
    writeln!(report, "agreements: {agreements}").unwrap();
    for (bucket, count) in &ruled {
        writeln!(report, "ruled {bucket}: {count}").unwrap();
    }
    writeln!(report, "differing: {}", differing.len()).unwrap();
    writeln!(report, "panics: {}", panics.len()).unwrap();
    writeln!(report, "top-level lets: not diffed (no body renderer yet)").unwrap();
    if !panics.is_empty() {
        writeln!(report, "\n== panics ==").unwrap();
        for line in &panics {
            writeln!(report, "{line}").unwrap();
        }
    }
    if !differing.is_empty() {
        writeln!(report, "\n== differing functions ==").unwrap();
        for (key, excerpt) in &differing {
            writeln!(report, "{key}").unwrap();
            write!(report, "{excerpt}").unwrap();
        }
    }

    assert_compiler2_snapshot!(super::SNAPSHOT_PATH, "s16_provider_sweep", report);
}
