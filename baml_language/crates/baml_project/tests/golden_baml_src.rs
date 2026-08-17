//! Golden-diff harness for compiler-behavior-preserving refactors: print
//! every diagnostic the full `baml_tests/baml_src` project produces, in the
//! collector's own deterministic order, as stable one-line records.
//!
//! Ignored by default (it cold-compiles a 48k-LOC project). Run explicitly,
//! capture, and diff across two checkouts:
//!
//! ```text
//! cargo test -p baml_project --test golden_baml_src -- --ignored --nocapture \
//!   | grep '^DIAG|' > after.txt
//! # ...switch code version, rebuild...
//! diff before.txt after.txt   # empty = behavior preserved
//! ```

use baml_project::ProjectDatabase;

#[test]
#[ignore = "golden-diff harness: cold-compiles baml_tests/baml_src; run explicitly and diff output across code versions"]
fn golden_baml_src_diagnostics() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../baml_tests/baml_src");
    let root = root.canonicalize().expect("baml_src exists");
    let files = baml_workspace::discover_baml_files(&root);
    assert!(
        files.len() > 50,
        "expected the full baml_src corpus, found {} files",
        files.len()
    );

    let mut db = ProjectDatabase::new();
    db.set_project_root(&root);
    for path in &files {
        let text = std::fs::read_to_string(path).expect("readable source file");
        db.add_or_update_file(path, &text);
    }

    let result = db.check();
    // One stable line per diagnostic: relative path, primary range, code,
    // message. The collector's sort is already a total order; printing in
    // that order keeps the diff byte-stable.
    #[allow(clippy::print_stdout)]
    for diag in &result.diagnostics {
        let (path, range) = match diag.primary_span() {
            Some(span) => {
                let path = result
                    .file_paths
                    .get(&span.file_id)
                    .map(|p| {
                        p.strip_prefix(&root)
                            .unwrap_or(p)
                            .display()
                            .to_string()
                    })
                    .unwrap_or_else(|| format!("file#{}", span.file_id.as_u32()));
                (
                    path,
                    format!("{}..{}", u32::from(span.range.start()), u32::from(span.range.end())),
                )
            }
            None => ("<no-span>".to_string(), String::new()),
        };
        println!(
            "DIAG|{}|{}|{:?}|{}|{}",
            path,
            range,
            diag.severity,
            diag.code(),
            diag.message.replace('\n', "\\n"),
        );
    }
    println!("DIAG-TOTAL|{}", result.diagnostics.len());
}
