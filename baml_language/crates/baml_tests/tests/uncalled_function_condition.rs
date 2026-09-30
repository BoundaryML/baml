//! CLI rendering coverage; diagnostic behavior is tested in native BAML under
//! `root.compiler.conditions.uncalled_functions`.

use std::{collections::HashMap, path::Path};

use baml_compiler_diagnostics::{RenderConfig, render_diagnostic};
use baml_db::{ProjectDatabase, collect_compiler2_diagnostics};
use baml_tests::engine::TestDbExt;

#[test]
fn named_function_labels_the_uncalled_reference_and_unreachable_else() {
    let source = r#"function condition() -> bool { false }

function main() -> string {
    if (condition) {
        "taken"
    } else {
        "unreachable"
    }
}
"#;
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("/uncalled-function"));
    let file = db.file(Path::new("/uncalled-function/main.baml"), source);
    let diagnostics: Vec<_> = collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| {
            diagnostic
                .primary_span()
                .is_some_and(|span| span.file_id == file.file_id(&db))
        })
        .collect();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let diagnostic = &diagnostics[0];
    let file_id = diagnostic.primary_span().unwrap().file_id;
    insta::assert_snapshot!(render_diagnostic(
        diagnostic,
        &HashMap::from([(file_id, source.to_string())]),
        &HashMap::from([(file_id, Path::new("main.baml").to_path_buf())]),
        &RenderConfig::test(),
    ));
}
