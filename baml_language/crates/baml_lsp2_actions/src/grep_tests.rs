//! Snapshot tests for `grep()` and `list_symbols()`.

use std::fmt::Write;

use crate::{grep::GrepMode, testing::ProjectTest};

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
fn grep_known_symbol_uses_semantic_mode() {
    let project = make_project();
    let result = project.grep("Point");
    assert_eq!(result.mode, GrepMode::Semantic);
    assert!(!result.descriptions.is_empty());
    assert!(result.text_matches.is_empty());
}

#[test]
fn grep_semantic_result_snapshot() {
    let project = make_project();
    let result = project.grep("Point");
    let mut output = String::new();
    for desc in &result.descriptions {
        output.push_str(&project.format_description(desc));
    }
    insta::assert_snapshot!(output);
}

#[test]
fn grep_unknown_pattern_uses_text_mode() {
    let project = make_project();
    let result = project.grep("xyz_no_match");
    assert_eq!(result.mode, GrepMode::TextSearch);
    assert!(result.descriptions.is_empty());
    assert!(result.text_matches.is_empty());
}

#[test]
fn grep_text_search_with_matches() {
    let project = make_project();
    // "return" appears in source but isn't a symbol name
    let result = project.grep("return");
    assert_eq!(result.mode, GrepMode::TextSearch);
    assert!(!result.text_matches.is_empty());

    let mut output = String::new();
    for m in &result.text_matches {
        output.push_str(&project.format_text_match(m));
        output.push('\n');
    }
    insta::assert_snapshot!(output);
}

#[test]
fn grep_case_insensitive_text_search() {
    let project = make_project();
    // "point" lowercase should find matches in text mode since describe is case-sensitive
    let result = project.grep_case_insensitive("point");
    // Should either go semantic (if case-insensitive matching applies) or text search
    let mut output = String::new();
    if result.mode == GrepMode::Semantic {
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
fn grep_enum_symbol() {
    let project = make_project();
    let result = project.grep("Color");
    assert_eq!(result.mode, GrepMode::Semantic);
    let mut output = String::new();
    for desc in &result.descriptions {
        output.push_str(&project.format_description(desc));
    }
    insta::assert_snapshot!(output);
}
