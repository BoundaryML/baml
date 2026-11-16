//! Every `.baml` file the repo owns lexes without panicking and reproduces
//! its text exactly from its tokens — the lossless-substrate property the
//! tree relies on, checked on real inputs rather than only on unit cases.

use std::path::Path;

use baml_syntax::lexer::lex;

const ROOTS: [&str; 4] = [
    "../baml_tests/baml_src",
    "../baml_tests/projects",
    "../baml_builtins2/baml_std",
    "../../tools/type_quiz",
];

#[test]
fn every_corpus_file_round_trips() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = 0usize;
    let mut tokens_total = 0usize;
    for root in ROOTS {
        let root = crate_dir.join(root);
        for entry in walkdir::WalkDir::new(&root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        {
            let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
            if entry.path().extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path())
                .unwrap_or_else(|e| panic!("reading {}: {e}", entry.path().display()));
            let tokens = lex(&text);
            let joined: String = tokens.iter().map(|i| tokens.text(&text, i)).collect();
            assert_eq!(
                joined,
                text,
                "{} does not round-trip",
                entry.path().display()
            );
            for i in tokens.iter() {
                assert!(
                    !tokens.range(i).is_empty(),
                    "{}: empty token",
                    entry.path().display()
                );
            }
            files += 1;
            tokens_total += tokens.len();
        }
    }
    assert!(
        files > 500,
        "expected the whole corpus, found {files} files"
    );
    assert!(tokens_total > 0);
}
