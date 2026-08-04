//! S15: the pre-cutover differential sweep.
//!
//! Runs EVERY file of the living runtime corpus (`baml_tests/baml_src/`,
//! code written without hir_ty in mind) through both engines and
//! aggregates every node where they disagree into a classified report -
//! the DIVERGENCE LEDGER. Buckets:
//!
//! - `conflict`: both engines typed the node, differently. Grouped by the
//!   exact `(hir_ty, tir)` render pair with example sites - the raw
//!   material for classification (spec-mandated improvement / hir_ty bug
//!   / semantics ruling). Nothing here is "fixed" silently: the snapshot
//!   IS the review artifact.
//! - `one-sided`: only one engine recorded a type at the range (coverage
//!   differences, counted but not itemized).
//! - `hir_ty error channel`: mismatches/non-exhaustive entries on a
//!   corpus that TIR compiles clean - candidate engine bugs or
//!   stricter-by-spec verdicts, every entry itemized.
//! - `panic`: a file whose inference panicked in either engine -
//!   itemized; uncharted constructs are exactly what the sweep hunts.
//!
//! The exit criterion for cutover (S16): every conflict group is either
//! matched to a documented spec-ahead divergence or resolved; the ledger
//! then becomes the cutover changelog.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::harness::{
    NodeKind, collect_hir_ty_error_channel, collect_hir_ty_nodes, collect_tir_nodes,
    tir_error_diagnostics,
};

fn baml_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src")
}

fn read_corpus_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            read_corpus_files(root, &path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .expect("strip corpus prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path)
                .expect("read corpus file")
                .replace("\r\n", "\n");
            out.push((rel, content));
        }
    }
}

/// One divergence group: a distinct `(hir_ty, tir)` render pair.
#[derive(Default)]
struct ConflictGroup {
    count: usize,
    /// Up to two `file:start..end 'text'` example sites.
    examples: Vec<String>,
}

#[test]
fn s15_sweep_baml_src() {
    let root = baml_src_dir();
    let mut files = Vec::new();
    read_corpus_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!files.is_empty(), "no corpus files found");

    let mut db = crate::compiler2_tir::support::make_db();
    let loaded: Vec<(String, String, baml_base::SourceFile)> = files
        .into_iter()
        .map(|(rel, content)| {
            let file = db.add_file(&rel, &content);
            (rel, content, file)
        })
        .collect();

    let mut nodes_compared = 0usize;
    let mut agreements = 0usize;
    let mut one_sided = 0usize;
    let mut conflicts: BTreeMap<(String, String), ConflictGroup> = BTreeMap::new();
    let mut channel_entries: Vec<String> = Vec::new();
    let mut tir_diagnostics: Vec<String> = Vec::new();
    let mut panics: Vec<String> = Vec::new();

    for (rel, content, file) in &loaded {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let hir = collect_hir_ty_nodes(&db, *file, content);
            let tir = collect_tir_nodes(&db, *file, content);
            let channel = collect_hir_ty_error_channel(&db, *file);
            let diags = tir_error_diagnostics(&db, *file);
            (hir, tir, channel, diags)
        }));
        let (hir, tir, channel, diags) = match outcome {
            Ok(parts) => parts,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("<non-string panic>");
                panics.push(format!("{rel}: {}", first_line(msg)));
                continue;
            }
        };

        for diag in diags {
            tir_diagnostics.push(format!("{rel}: {}", first_line(&diag)));
        }
        for (&(start, end), rendered) in &channel.mismatches {
            for entry in rendered {
                channel_entries.push(format!(
                    "{rel}:{start}..{end} `{}`: {entry}",
                    snippet(content, start, end)
                ));
            }
        }
        for range in &channel.non_exhaustive {
            let (start, end) = (u32::from(range.start()), u32::from(range.end()));
            channel_entries.push(format!(
                "{rel}:{start}..{end} `{}`: non-exhaustive",
                snippet(content, start, end)
            ));
        }

        // Merge nodes per range, exactly the dump's discipline.
        let mut merged: BTreeMap<(u32, u32), (Vec<String>, Vec<String>)> = BTreeMap::new();
        for (nodes, side) in [(&hir, 0usize), (&tir, 1usize)] {
            for node in nodes.iter() {
                if node.kind == NodeKind::BindingName {
                    continue;
                }
                let entry = merged
                    .entry((u32::from(node.range.start()), u32::from(node.range.end())))
                    .or_default();
                let list = if side == 0 { &mut entry.0 } else { &mut entry.1 };
                if !list.contains(&node.ty) {
                    list.push(node.ty.clone());
                }
            }
        }
        for ((start, end), (mut h, mut t)) in merged {
            h.sort();
            t.sort();
            nodes_compared += 1;
            if h == t {
                agreements += 1;
            } else if h.is_empty() || t.is_empty() {
                one_sided += 1;
            } else {
                let group = conflicts
                    .entry((h.join(" / "), t.join(" / ")))
                    .or_default();
                group.count += 1;
                if group.examples.len() < 2 {
                    group
                        .examples
                        .push(format!("{rel}:{start}..{end} `{}`", snippet(content, start, end)));
                }
            }
        }
    }

    let mut report = String::new();
    use std::fmt::Write as _;
    let conflict_total: usize = conflicts.values().map(|group| group.count).sum();
    let _ = writeln!(report, "files: {}", loaded.len());
    let _ = writeln!(report, "nodes compared: {nodes_compared}");
    let _ = writeln!(report, "agreements: {agreements}");
    let _ = writeln!(report, "one-sided (coverage): {one_sided}");
    let _ = writeln!(
        report,
        "conflicts: {conflict_total} across {} distinct pairs",
        conflicts.len()
    );
    let _ = writeln!(report, "hir_ty error-channel entries: {}", channel_entries.len());
    let _ = writeln!(report, "tir diagnostics: {}", tir_diagnostics.len());
    let _ = writeln!(report, "panics: {}", panics.len());

    if !panics.is_empty() {
        let _ = writeln!(report, "\n== panics ==");
        for line in &panics {
            let _ = writeln!(report, "{line}");
        }
    }
    if !tir_diagnostics.is_empty() {
        let _ = writeln!(report, "\n== tir diagnostics ==");
        for line in &tir_diagnostics {
            let _ = writeln!(report, "{line}");
        }
    }
    if !channel_entries.is_empty() {
        let _ = writeln!(report, "\n== hir_ty error channel ==");
        for line in &channel_entries {
            let _ = writeln!(report, "{line}");
        }
    }
    if !conflicts.is_empty() {
        let _ = writeln!(report, "\n== conflicts by (hir_ty, tir) pair ==");
        let mut ordered: Vec<_> = conflicts.iter().collect();
        ordered.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        for ((h, t), group) in ordered {
            let _ = writeln!(report, "[{}x] hir_ty=[{h}] tir=[{t}]", group.count);
            for example in &group.examples {
                let _ = writeln!(report, "    at {example}");
            }
        }
    }

    insta::assert_snapshot!("s15_sweep_baml_src", report);
}

fn snippet(content: &str, start: u32, end: u32) -> String {
    let raw = &content[start as usize..end as usize];
    let flat = raw.replace('\n', "\\n");
    if flat.len() <= 24 {
        flat
    } else {
        format!("{}...{}", &flat[..10], &flat[flat.len() - 11..])
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}
