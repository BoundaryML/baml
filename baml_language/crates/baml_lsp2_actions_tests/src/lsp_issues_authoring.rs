use std::path::Path;

use baml_lsp2_actions::{CompletionInsertTextFormat, CompletionKind, completions_at};
use baml_project::ProjectDatabase;
use text_size::TextSize;

#[test]
fn function_completion_is_a_snippet() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let source = "fun";
    let file = db.add_or_update_file(Path::new("b974_function_completion.baml"), source);

    let completions = completions_at(&db, file, TextSize::from(source.len() as u32));
    for label in [
        "class",
        "enum",
        "function",
        "client",
        "test",
        "retry_policy",
        "type",
        "interface",
        "implements",
    ] {
        let item = completions
            .iter()
            .find(|item| item.label == label)
            .unwrap_or_else(|| panic!("top-level completion should offer `{label}`"));
        assert!(
            item.insert_text.is_some(),
            "`{label}` should insert a declaration skeleton"
        );
        assert_eq!(
            item.insert_text_format,
            CompletionInsertTextFormat::Snippet,
            "`{label}` should preserve snippet tabstops"
        );
    }

    let function = completions
        .iter()
        .find(|item| item.label == "function")
        .expect("top-level completion should offer `function`");

    assert_eq!(function.kind, CompletionKind::Keyword);
    assert_eq!(
        function.insert_text.as_deref(),
        Some("function ${1:Name}(${2}) -> ${3:string} {\n  $0\n}")
    );
    assert_eq!(
        function.insert_text_format,
        CompletionInsertTextFormat::Snippet
    );

    let test = completions
        .iter()
        .find(|item| item.label == "test")
        .expect("top-level completion should offer `test`");
    assert_eq!(
        test.insert_text.as_deref(),
        Some("test \"${1:test name}\" {\n  $0\n}")
    );
}
