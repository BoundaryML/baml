use std::path::Path;

use baml_compiler_diagnostics::DiagnosticId;
use baml_lsp2_actions::check::check_file;
use baml_project::ProjectDatabase;
use text_size::{TextRange, TextSize};

#[test]
fn unresolved_throws_types_are_reported_once_at_the_signature() {
    let source = r#"function unresolved_throws_empty() -> string throws MissingEmpty {
  `x`
}

function unresolved_throws_with_body_annotations() -> string throws MissingInBounds {
  let a: int = 1
  let b: string = `b`
  let c: bool = true
  let d: float = 4.0
  let e: int[] = [5]
  let f: map<string, int> = { "f": 6 }
  `${a}${b}${c}${d}${e}${f}`
}
"#;
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let file = db.add_or_update_file(Path::new("b1607.baml"), source);

    let diagnostics = check_file(&db, file);
    let unresolved: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.id == DiagnosticId::UnknownType)
        .collect();
    assert_eq!(unresolved.len(), 2, "each signature should report once");

    for (diagnostic, missing) in unresolved
        .into_iter()
        .zip(["MissingEmpty", "MissingInBounds"])
    {
        assert_eq!(diagnostic.message, format!("unresolved type: {missing}"));
        let start = source.find(missing).expect("missing type in source") as u32;
        let expected = TextRange::at(TextSize::new(start), TextSize::new(missing.len() as u32));
        let primary = diagnostic
            .annotations
            .iter()
            .find(|annotation| annotation.is_primary)
            .expect("primary annotation");
        assert_eq!(primary.span.range, expected);
    }
}

#[test]
fn unresolved_type_in_parameter_default_has_its_source_span() {
    let source = r#"function parameter_default(
  value: int = { let typed: MissingDefault = 1; 1 }
) -> int {
  value
}
"#;
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let file = db.add_or_update_file(Path::new("b1607-default.baml"), source);

    let diagnostics = check_file(&db, file);
    let unresolved: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.id == DiagnosticId::UnknownType)
        .collect();
    assert_eq!(unresolved.len(), 1);

    let diagnostic = unresolved[0];
    let missing = "MissingDefault";
    let start = source.find(missing).expect("missing type in source") as u32;
    let expected = TextRange::at(TextSize::new(start), TextSize::new(missing.len() as u32));
    let primary = diagnostic
        .annotations
        .iter()
        .find(|annotation| annotation.is_primary)
        .expect("primary annotation");
    assert_eq!(primary.span.range, expected);
}
