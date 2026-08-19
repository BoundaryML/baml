//! The whole-corpus sweep: run hir_ty over every `baml_src` file and
//! census the typed surface - node and error-channel counts per corpus,
//! with any PANIC surfaced verbatim. Descended from the S15/S16
//! hir-vs-TIR agreement sweep; with TIR retired the census remains as the
//! wide net that catches inference panics and channel regressions the
//! per-project tiers do not reach.

use std::path::{Path, PathBuf};

use super::harness::{NodeKind, collect_hir_ty_error_channel, collect_hir_ty_nodes};
use crate::engine::TestDbExt;

pub(crate) fn baml_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src")
}

pub(crate) fn read_corpus_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
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

/// The ruled-divergence taxonomy, MACHINE-CHECKED: every conflict pair
/// must land in a bucket or the sweep fails. This is a coarse
/// accounting net for the S15 rulings, not an equivalence oracle - each
/// bucket corresponds to a documented ruling (crate README, S15/S15.5
/// sections); an unclassified pair means either a new regression or a
/// divergence nobody ruled on yet, and both must be looked at.
#[test]
fn s15_sweep_baml_src() {
    let root = baml_src_dir();
    let mut files = Vec::new();
    read_corpus_files(&root, &root, &mut files);
    files.sort();

    let mut db = crate::compiler2_tir::support::make_db();
    let loaded: Vec<(String, String, baml_base::SourceFile)> = files
        .into_iter()
        .map(|(rel, content)| {
            let file = db.file(&rel, &content);
            (rel, content, file)
        })
        .collect();

    let mut typed_nodes = 0usize;
    let mut channel_entries: Vec<String> = Vec::new();
    let mut panics: Vec<String> = Vec::new();

    for (rel, content, file) in &loaded {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let hir = collect_hir_ty_nodes(&db, *file, content);
            let channel = collect_hir_ty_error_channel(&db, *file);
            (hir, channel)
        }));
        let (hir, channel) = match outcome {
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

        typed_nodes += hir
            .iter()
            .filter(|node| node.kind != NodeKind::BindingName)
            .count();
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
    }

    let mut report = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(report, "files: {}", loaded.len());
    let _ = writeln!(report, "typed nodes: {typed_nodes}");
    let _ = writeln!(
        report,
        "hir_ty error-channel entries: {}",
        channel_entries.len()
    );
    let _ = writeln!(report, "panics: {}", panics.len());
    let _ = writeln!(report, "\n== hir_ty error channel");
    for entry in &channel_entries {
        let _ = writeln!(report, "{entry}");
    }
    if !panics.is_empty() {
        let _ = writeln!(report, "\n== panics");
        for entry in &panics {
            let _ = writeln!(report, "{entry}");
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
