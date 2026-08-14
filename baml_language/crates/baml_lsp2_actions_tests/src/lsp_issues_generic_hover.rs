use std::path::Path;

use baml_lsp2_actions::completions_at;
use baml_project::ProjectDatabase;
use text_size::TextSize;

#[test]
fn instantiated_field_completion_uses_realized_type() {
    let marked = r#"class Lorem<T> {
  a: T
}

function lorem(a: Lorem<int>) -> void {
  a.<[CURSOR]
}
"#;
    let cursor = marked.find("<[CURSOR]").expect("cursor marker");
    let source = marked.replace("<[CURSOR]", "");
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let file = db.add_or_update_file(
        Path::new("b982_instantiated_field_completion.baml"),
        &source,
    );

    let completions = completions_at(&db, file, TextSize::from(cursor as u32));
    let field = completions
        .iter()
        .find(|item| item.label == "a")
        .expect("field completion should offer `a`");

    assert_eq!(
        field.detail.as_deref(),
        Some("int"),
        "B-982: completion detail should use the receiver-instantiated field type"
    );
}
