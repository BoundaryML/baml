//! Snapshot tests for `search_project()` and `list_symbols()`.

use std::fmt::Write;

use crate::{
    DefinitionKind,
    project_search::{ProjectSearchMode, ProjectSearchOptions, search_text},
    testing::ProjectTest,
};

fn make_project() -> ProjectTest {
    let mut builder = ProjectTest::builder();
    builder.source(
        "types.baml",
        r#"
class Point {
    x int
    y int
}

enum Color {
    Red,
    Green,
    Blue,
}
"#,
    );
    builder.source(
        "funcs.baml",
        r#"
function MakePoint(x: int, y: int) -> Point {
    return Point { x: x, y: y };
}

function ReadColor(c: Color) -> string {
    match (c) {
        Red => "red"
        Green => "green"
        Blue => "blue"
    }
}
"#,
    );
    builder.build()
}

#[test]
fn known_symbol_uses_semantic_mode() {
    let project = make_project();
    let result = project.search_project("Point");
    assert_eq!(result.mode, ProjectSearchMode::Semantic);
    assert!(!result.descriptions.is_empty());
    assert!(result.text_matches.is_empty());
}

#[test]
fn semantic_result_snapshot() {
    let project = make_project();
    let result = project.search_project("Point");
    let mut output = String::new();
    for desc in &result.descriptions {
        output.push_str(&project.format_description(desc));
    }
    insta::assert_snapshot!(output);
}

#[test]
fn unknown_pattern_uses_text_mode() {
    let project = make_project();
    let result = project.search_project("xyz_no_match");
    assert_eq!(result.mode, ProjectSearchMode::TextSearch);
    assert!(result.descriptions.is_empty());
    assert!(result.text_matches.is_empty());
}

#[test]
fn text_search_with_matches() {
    let project = make_project();
    // "return" appears in source but isn't a symbol name
    let result = project.search_project("return");
    assert_eq!(result.mode, ProjectSearchMode::TextSearch);
    assert!(!result.text_matches.is_empty());

    let mut output = String::new();
    for m in &result.text_matches {
        output.push_str(&project.format_text_match(m));
        output.push('\n');
    }
    insta::assert_snapshot!(output);
}

#[test]
fn case_insensitive_text_search() {
    let project = make_project();
    // "point" lowercase should find matches in text mode since describe is case-sensitive
    let result = project.grep_case_insensitive("point");
    // Should either go semantic (if case-insensitive matching applies) or text search
    let mut output = String::new();
    if result.mode == ProjectSearchMode::Semantic {
        for desc in &result.descriptions {
            output.push_str(&project.format_description(desc));
        }
    } else {
        for m in &result.text_matches {
            output.push_str(&project.format_text_match(m));
            output.push('\n');
        }
    }
    insta::assert_snapshot!(output);
}

#[test]
fn list_symbols_snapshot() {
    let project = make_project();
    let symbols = project.list_symbols();
    let mut output = String::new();
    for sym in &symbols {
        let filename = sym
            .file
            .path(&project.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        writeln!(output, "{:<20} {:<12} {}", sym.name, sym.kind, filename).unwrap();
    }
    insta::assert_snapshot!(output);
}

#[test]
fn enum_symbol_search() {
    let project = make_project();
    let result = project.search_project("Color");
    assert_eq!(result.mode, ProjectSearchMode::Semantic);
    let mut output = String::new();
    for desc in &result.descriptions {
        output.push_str(&project.format_description(desc));
    }
    insta::assert_snapshot!(output);
}

#[test]
fn text_search_kind_filter_excludes_other_kinds_and_unannotated_text() {
    let project = make_project();
    let class_only = [DefinitionKind::Class];
    let opts = ProjectSearchOptions {
        pattern: "Point",
        ignore_case: false,
        kind_filter: &class_only,
    };

    let matches = search_text(&project.db, &project.files, &opts);

    assert!(!matches.is_empty());
    assert!(matches.iter().all(|text_match| matches!(
        text_match.annotation,
        Some(crate::MatchAnnotation::Definition {
            kind: DefinitionKind::Class,
            ..
        }) | Some(crate::MatchAnnotation::Reference {
            target_kind: DefinitionKind::Class,
            ..
        })
    )));

    let function_only = [DefinitionKind::Function];
    let opts = ProjectSearchOptions {
        pattern: "return",
        ignore_case: false,
        kind_filter: &function_only,
    };
    assert!(search_text(&project.db, &project.files, &opts).is_empty());
}
