//! The IDE over global variables: a top-level `let` reads as the binding it
//! is (keyword, name, type, docs), navigates and finds references through
//! every spelling of it — bare, namespace-qualified, `root.`-qualified — and
//! its initializer is a body like any function's, so the features that read a
//! body's locals and members answer inside it too.

use crate::{
    completion::completions,
    definition::definition_at,
    info::{TypeInfo, type_at},
    resolve::Location,
    test_support::CursorTest,
    usages::usages_at,
};

fn hover(test: &CursorTest) -> TypeInfo {
    type_at(&test.db, test.cursor.file, test.cursor.offset)
        .unwrap_or_else(|| panic!("expected type info at the fixture cursor"))
}

fn usages(test: &CursorTest) -> Vec<String> {
    let mut found: Vec<String> = usages_at(&test.db, test.cursor.file, test.cursor.offset)
        .iter()
        .map(|location| describe(test, location))
        .collect();
    found.sort();
    found
}

fn describe(test: &CursorTest, location: &Location) -> String {
    test.format_file_range_with_text(location.file, location.range)
}

#[test]
fn hover_on_a_global_shows_its_annotation_and_docs() {
    let test = CursorTest::new(
        r#"/// How many times to retry.
let MAX_RETRIES: int = 3;

function f() -> int {
    MAX_RE<[CURSOR]TRIES
}
"#,
    );
    let info = hover(&test);
    assert_eq!(info.to_hover_block(), "let MAX_RETRIES: int");
    assert_eq!(info.docs(), Some("How many times to retry."));
}

#[test]
fn hover_on_an_unannotated_global_shows_the_widened_initializer_type() {
    let test = CursorTest::new(
        r#"let LIMIT = 5;
let NAMES = ["a", "b"];

function f() -> int {
    LIM<[CURSOR]IT + NAMES.length()
}
"#,
    );
    assert_eq!(hover(&test).to_hover_block(), "let LIMIT: int");
}

#[test]
fn hover_on_a_client_declaration_names_it_a_client() {
    let test = CursorTest::new(
        r#"client Model = openai.ChatClient.new(model = "gpt-4o-mini");

function f() -> string {
    Mo<[CURSOR]del.model
}
"#,
    );
    assert!(
        hover(&test).to_hover_block().starts_with("client Model: "),
        "{}",
        hover(&test).to_hover_block()
    );
}

#[test]
fn goto_definition_follows_a_namespace_qualified_global() {
    let mut builder = CursorTest::builder();
    builder.source(
        "main.baml",
        r#"function f() -> int {
    settings.TIME<[CURSOR]OUT
}
"#,
    );
    builder.source("ns_settings/settings.baml", "let TIMEOUT = 30;\n");
    let test = builder.build();
    let location = definition_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("a qualified global navigates to its declaration");
    assert_eq!(describe(&test, &location), "settings.baml:1:5 -> TIMEOUT");
}

#[test]
fn references_include_every_spelling_and_skip_shadowing_locals() {
    let mut builder = CursorTest::builder();
    builder.source(
        "main.baml",
        r#"let LIMIT<[CURSOR] = 3;

function direct() -> int {
    LIMIT
}

function shadowed() -> int {
    let LIMIT = 100;
    LIMIT
}

function parameter(LIMIT: int) -> int {
    LIMIT
}
"#,
    );
    builder.source("ns_child/child.baml", "let DOUBLE = root.LIMIT * 2;\n");
    let test = builder.build();
    let found = usages(&test);
    assert_eq!(
        found,
        vec![
            "child.baml:1:19 -> LIMIT".to_string(),
            "main.baml:1:5 -> LIMIT".to_string(),
            "main.baml:4:5 -> LIMIT".to_string(),
        ],
    );
}

#[test]
fn references_to_a_child_namespace_global_include_its_qualified_reads() {
    let mut builder = CursorTest::builder();
    builder.source(
        "main.baml",
        r#"function f() -> int {
    settings.TIMEOUT + 1
}
"#,
    );
    builder.source(
        "ns_settings/settings.baml",
        "let TIME<[CURSOR]OUT = 30;\nlet BUDGET = TIMEOUT * 3;\n",
    );
    let test = builder.build();
    assert_eq!(
        usages(&test),
        vec![
            "main.baml:2:14 -> TIMEOUT".to_string(),
            "settings.baml:1:5 -> TIMEOUT".to_string(),
            "settings.baml:2:14 -> TIMEOUT".to_string(),
        ],
    );
}

/// Like a function local's, the references are its reads (the declaration is
/// the definition, which navigation reaches).
#[test]
fn a_local_inside_a_global_initializer_hovers_and_has_references() {
    let test = CursorTest::new(
        r#"let TOTAL = {
    let part = 1;
    pa<[CURSOR]rt + part
};
"#,
    );
    assert_eq!(hover(&test).to_hover_block(), "let part: int");
    assert_eq!(
        usages(&test),
        vec![
            "test.baml:3:12 -> part".to_string(),
            "test.baml:3:5 -> part".to_string(),
        ],
    );
}

#[test]
fn goto_definition_on_a_local_inside_a_global_initializer() {
    let test = CursorTest::new(
        r#"let TOTAL = {
    let part = 1;
    pa<[CURSOR]rt + 2
};
"#,
    );
    let location = definition_at(&test.db, test.cursor.file, test.cursor.offset)
        .expect("a local of an initializer navigates to its declaration");
    assert_eq!(describe(&test, &location), "test.baml:2:9 -> part");
}

#[test]
fn a_member_inside_a_global_initializer_resolves() {
    let test = CursorTest::new(
        r#"class Settings {
    retries int
}

let SETTINGS = Settings { retries: 2 };
let RETRIES = SETTINGS.retr<[CURSOR]ies;
"#,
    );
    let TypeInfo::LocalVar { name, ty, .. } = hover(&test) else {
        panic!("a field read hovers as the field");
    };
    assert_eq!((name.as_str(), ty.as_str()), ("retries", "int"));
}

#[test]
fn completion_after_a_global_offers_its_members() {
    let test = CursorTest::new(
        r#"let NAMES: string[] = ["a"];

function f() -> int {
    NAMES.<[CURSOR]
    0
}
"#,
    );
    let items = completions(&test.db, test.cursor.file, test.cursor.offset);
    assert!(
        items.iter().any(|item| item.label == "length"),
        "{:?}",
        items.iter().map(|item| &item.label).collect::<Vec<_>>()
    );
}

#[test]
fn completion_inside_a_global_initializer_offers_members() {
    let test = CursorTest::new(
        r#"let NAMES: string[] = ["a"];
let COUNT = NAMES.<[CURSOR];
"#,
    );
    let items = completions(&test.db, test.cursor.file, test.cursor.offset);
    assert!(
        items.iter().any(|item| item.label == "length"),
        "{:?}",
        items.iter().map(|item| &item.label).collect::<Vec<_>>()
    );
}

fn type_hints(test: &CursorTest) -> Vec<String> {
    crate::annotations::file_annotations(&test.db, test.cursor.file)
        .iter()
        .filter(|hint| hint.kind == crate::annotations::AnnotationKind::Type)
        .map(|hint| {
            let text = test.cursor.file.text(&test.db);
            let line = text[..usize::from(hint.offset)].matches('\n').count() + 1;
            format!("{line}{}", hint.label)
        })
        .collect()
}

#[test]
fn an_unannotated_global_gets_a_type_hint_like_a_local() {
    let test = CursorTest::new(
        r#"function count() -> int {
    3
}

let FROM_CALL = count();
let ANNOTATED: int = count();
let LITERAL<[CURSOR] = 4;
let FROM_BLOCK = {
    let inner = count();
    inner
};
"#,
    );
    // The call's type is not obvious from its spelling; an annotation, a
    // literal, and a local's own hint inside an initializer behave as they
    // do in a body.
    assert_eq!(
        type_hints(&test),
        vec![
            "5: int".to_string(),
            "8: int".to_string(),
            "9: int".to_string()
        ],
    );
}

#[test]
fn global_tokens_are_readonly_variables_and_qualifiers_are_namespaces() {
    use crate::tokens::{ModifierSet, SemanticTokenType, semantic_tokens};

    let main = r#"let LIMIT = 3;

function f() -> int {
    let local = 1;
    LIMIT + settings.TIMEOUT + local
}
"#;
    let mut builder = CursorTest::builder();
    builder.source("main.baml", &format!("{main}<[CURSOR]"));
    builder.source("ns_settings/settings.baml", "let TIMEOUT = 30;\n");
    let test = builder.build();
    let tokens = semantic_tokens(&test.db, test.cursor.file);
    let class_at = |needle: &str, nth: usize| {
        let offset = main
            .match_indices(needle)
            .nth(nth)
            .map(|(at, _)| text_size::TextSize::try_from(at).unwrap())
            .unwrap_or_else(|| panic!("`{needle}` #{nth} is in the fixture"));
        tokens
            .iter()
            .find(|token| token.range.start() == offset)
            .map(|token| (token.token_type, token.modifiers))
            .unwrap_or_else(|| panic!("`{needle}` #{nth} is tokenized"))
    };
    assert_eq!(
        class_at("LIMIT", 0),
        (
            SemanticTokenType::Variable,
            ModifierSet::DECLARATION | ModifierSet::READONLY
        )
    );
    assert_eq!(
        class_at("LIMIT", 1),
        (SemanticTokenType::Variable, ModifierSet::READONLY)
    );
    assert_eq!(
        class_at("settings", 0),
        (SemanticTokenType::Namespace, ModifierSet::empty())
    );
    assert_eq!(
        class_at("TIMEOUT", 0),
        (SemanticTokenType::Variable, ModifierSet::READONLY)
    );
    // A local is an ordinary, reassignable variable.
    assert_eq!(
        class_at("local", 1),
        (SemanticTokenType::Variable, ModifierSet::empty())
    );
}
