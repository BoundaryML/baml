//! The IDE over a package served from its interface: every feature that
//! reads a member declaration answers for an exported row exactly as for a
//! source item — completion detail, semantic tokens, hover — except
//! navigation, since a row has no span in this database by construction.

use std::path::Path;

use baml_base::SourceFile;
use baml_db::ProjectDatabase;
use text_size::TextSize;

use crate::{
    completion::{Completion, CompletionKind, completions},
    definition::definition_at,
    info::{TypeInfo, type_at},
    test_support::{TestDbExt, export_blob},
    tokens::{SemanticTokenType, semantic_tokens},
};

const LIBRARY: &str = r#"
function free_fn(x: int) -> int throws never {
    x
}

class Widget {
    size int

    function describe(self) -> string throws never {
        "widget"
    }
}

interface Greeter {
    function greet(self) -> string throws never {
        "hi"
    }
}
"#;

const CONSUMER: &str = r#"
function use_widget(w: app.Widget, g: app.Greeter) -> int throws never {
    let n = app.free_fn(1);
    let s = w.describe();
    let t = g.greet();
    n + w.size
}
"#;

/// A workspace with the library MOUNTED as `app` from its exported
/// interface (no source), plus one consumer file with `source`.
fn mounted(source: &str) -> (ProjectDatabase, SourceFile) {
    let blob = export_blob("app", LIBRARY);
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("/ide-mounted"));
    db.mount("app", blob);
    let file = db.file(Path::new("/ide-mounted/main.baml"), source);
    (db, file)
}

/// An offset strictly inside the first occurrence of `needle` in `source`.
fn inside(source: &str, needle: &str) -> TextSize {
    let start = source
        .find(needle)
        .unwrap_or_else(|| unreachable!("`{needle}` is in the fixture"));
    TextSize::from(u32::try_from(start + 1).expect("fixture offsets fit u32"))
}

fn offered<'a>(items: &'a [Completion], label: &str) -> &'a Completion {
    items
        .iter()
        .find(|item| item.label == label)
        .unwrap_or_else(|| {
            let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
            panic!("`{label}` is offered; got {labels:?}")
        })
}

#[test]
fn completion_on_a_mounted_class_value_renders_its_members() {
    let source = "function f(w: app.Widget) -> int throws never {\n    w.\n}\n";
    let (db, file) = mounted(source);
    let offset = TextSize::from(u32::try_from(source.find("w.").unwrap() + 2).unwrap());
    let items = completions(&db, file, offset);

    let describe = offered(&items, "describe");
    assert!(
        matches!(describe.kind, CompletionKind::Method),
        "a row method completes as a method"
    );
    let detail = describe.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("-> string") && !detail.contains("self"),
        "an instance completion renders the row's signature with `self` hidden: {detail:?}"
    );

    let size = offered(&items, "size");
    assert!(matches!(size.kind, CompletionKind::Field));
    assert_eq!(
        size.detail.as_deref(),
        Some("int"),
        "a row field completes with its type"
    );
}

#[test]
fn completion_on_a_mounted_interface_value_renders_its_default_method() {
    let source = "function f(g: app.Greeter) -> int throws never {\n    g.\n}\n";
    let (db, file) = mounted(source);
    let offset = TextSize::from(u32::try_from(source.find("g.").unwrap() + 2).unwrap());
    let items = completions(&db, file, offset);

    let greet = offered(&items, "greet");
    assert!(matches!(greet.kind, CompletionKind::Method));
    let detail = greet.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("-> string"),
        "an interface row's default method renders its signature: {detail:?}"
    );
}

#[test]
fn semantic_tokens_classify_mounted_members_by_dispatch_mode() {
    let (db, file) = mounted(CONSUMER);
    let tokens = semantic_tokens(&db, file);
    let token_type = |needle: &str| {
        let offset = inside(CONSUMER, needle);
        tokens
            .iter()
            .find(|token| token.range.contains(offset))
            .unwrap_or_else(|| unreachable!("`{needle}` is tokenized"))
            .token_type
    };
    assert_eq!(token_type("free_fn"), SemanticTokenType::Function);
    assert_eq!(token_type("describe"), SemanticTokenType::Method);
    assert_eq!(token_type("greet"), SemanticTokenType::Method);
    assert_eq!(token_type("size"), SemanticTokenType::Property);
}

#[test]
fn goto_definition_on_a_mounted_member_navigates_nowhere() {
    // Pins the deliberate rule: a row has no `SourceFile` and no span in
    // this database, so there is nothing to navigate to.
    let (db, file) = mounted(CONSUMER);
    for needle in ["describe", "greet", "size"] {
        assert!(
            definition_at(&db, file, inside(CONSUMER, needle)).is_none(),
            "`{needle}` has no definition site in this database"
        );
    }
}

#[test]
fn hover_on_a_mounted_method_renders_its_signature() {
    let (db, file) = mounted(CONSUMER);
    let Some(TypeInfo::Symbol {
        declaration,
        owner,
        docstring,
    }) = type_at(&db, file, inside(CONSUMER, "describe"))
    else {
        panic!("a mounted method hovers as a symbol");
    };
    assert!(
        declaration.contains("describe") && declaration.contains("-> string"),
        "the row's signature: {declaration:?}"
    );
    assert!(
        owner
            .as_deref()
            .is_some_and(|owner| owner.contains("Widget")),
        "the owning class: {owner:?}"
    );
    assert_eq!(docstring, None, "rows carry no docstrings");
}

#[test]
fn hover_on_a_mounted_field_renders_its_type() {
    let (db, file) = mounted(CONSUMER);
    let Some(TypeInfo::LocalVar {
        name, ty, owner, ..
    }) = type_at(&db, file, inside(CONSUMER, "size"))
    else {
        panic!("a mounted field hovers as a field");
    };
    assert_eq!(name, "size");
    assert_eq!(ty, "int");
    assert!(
        owner
            .as_deref()
            .is_some_and(|owner| owner.contains("Widget")),
        "the owning class: {owner:?}"
    );
}
