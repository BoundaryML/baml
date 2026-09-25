//! The IDE over a package served from its interface: every feature that
//! reads a member declaration answers for an exported row exactly as for a
//! source item — completion detail, semantic tokens, hover — except
//! navigation, since a row has no span in this database by construction.

use std::path::Path;

use baml_base::SourceFile;
use baml_compiler2_hir::loc::DeclRef;
use baml_db::ProjectDatabase;
use text_size::TextSize;

use crate::{
    completion::{Completion, CompletionKind, completions},
    definition::definition_at,
    info::{TypeInfo, type_at},
    rename::{RenameError, rename},
    resolve::{SymbolTarget, symbol_at},
    test_support::{TestDbExt, export_blob},
    tokens::{SemanticTokenType, semantic_tokens},
    usages::usages_at,
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

/// A library whose rows carry what the IDE must render from rows alone: a
/// field docstring, a generic class, an impl-provided method, a free
/// function.
const ROW_LIBRARY: &str = r#"
class Widget {
    /// How wide.
    size int
}

class Box<T> {
    value T

    function get(self) -> T throws never {
        self.value
    }
}

interface Greeter {
    function greet(self) -> string throws never {
        "hi"
    }
}

class Tagged {
    implements Greeter {
        function greet(self) -> string throws never {
            "tagged"
        }
    }
}

function free_fn(x: int) -> int throws never {
    x
}

enum Status {
    Active
    Retired
}

interface Labeled {
    label string

    function tag(self) -> string throws never
}

class Card {
    label string

    implements Labeled {
        function tag(self) -> string throws never {
            self.label
        }
    }
}
"#;

/// A consumer over the enum, the interface field, and the required method.
const MEMBER_CONSUMER: &str = r#"
function pick(l: app.Labeled) -> string throws never {
    let status = app.Status.Active;
    l.tag()
}
"#;

const ROW_CONSUMER: &str = r#"
function build(n: int) -> app.Widget throws never {
    app.Widget { size: n }
}

function use_all(w: app.Widget, b: app.Box<int>, t: app.Tagged) -> int throws never {
    let n = app.free_fn(w.size);
    let s = t.greet();
    b.get() + n
}
"#;

/// A workspace with the library MOUNTED as `app` from its exported
/// interface (no source), plus one consumer file with `source`.
fn mounted(source: &str) -> (ProjectDatabase, SourceFile) {
    mounted_with(LIBRARY, source)
}

fn mounted_with(library: &str, source: &str) -> (ProjectDatabase, SourceFile) {
    let blob = export_blob("app", library);
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
    // Positive control: the rule is about rows, not about this database —
    // a source symbol in the same file still navigates.
    assert!(
        definition_at(&db, file, inside(CONSUMER, "use_widget")).is_some(),
        "the consumer's own function has a definition site"
    );
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
    assert_eq!(docstring, None, "callable rows carry no docstrings");
}

#[test]
fn completion_on_a_mounted_field_carries_its_docstring() {
    let source = "function f(w: app.Widget) -> int throws never {\n    w.\n}\n";
    let (db, file) = mounted_with(ROW_LIBRARY, source);
    let offset = TextSize::from(u32::try_from(source.find("w.").unwrap() + 2).unwrap());
    let items = completions(&db, file, offset);
    let size = offered(&items, "size");
    assert_eq!(
        size.documentation.as_deref(),
        Some("How wide."),
        "a field row carries its declaration's docstring"
    );
}

/// Hover owners render the same shape in both lanes: a class method under
/// the class's own type, an impl-provided method under the implementor.
#[test]
fn hover_owner_of_a_mounted_method_matches_the_source_lane() {
    let (db, file) = mounted_with(ROW_LIBRARY, ROW_CONSUMER);
    let owner_of = |needle: &str| match type_at(&db, file, inside(ROW_CONSUMER, needle)) {
        Some(TypeInfo::Symbol { owner, .. }) => owner,
        other => panic!("`{needle}` hovers as a symbol, got {other:?}"),
    };
    assert_eq!(owner_of("get()").as_deref(), Some("app.Box<T>"));
    assert_eq!(owner_of("greet()").as_deref(), Some("app.Tagged"));
}

/// BUG (pinned): a served package's free function has no symbol — `Item`
/// carries source definitions only. Flips when the definition lane gains
/// its provenance-total ref (PR-4's `DefinitionRef`); see `resolve.rs`.
#[test]
fn a_mounted_free_function_has_no_symbol_yet() {
    let (db, file) = mounted_with(ROW_LIBRARY, ROW_CONSUMER);
    assert!(symbol_at(&db, file, inside(ROW_CONSUMER, "free_fn")).is_none());
}

/// A constructor key of a served class is the same field every member
/// access resolves to — the exported row's, never a link stub's — so
/// references on the field find the key too.
#[test]
fn constructor_key_of_a_mounted_class_shares_the_field_identity() {
    let (db, file) = mounted_with(ROW_LIBRARY, ROW_CONSUMER);
    let key = inside(ROW_CONSUMER, "size: n");
    assert!(
        matches!(
            symbol_at(&db, file, key),
            Some(SymbolTarget::Field {
                class: DeclRef::External(_),
                field_index: 0,
            })
        ),
        "the key resolves to the exported row's field"
    );
    let usages = usages_at(&db, file, inside(ROW_CONSUMER, "size)"));
    assert!(
        usages.iter().any(|location| location.range.contains(key)),
        "references on the field include the constructor key: {usages:?}"
    );
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

/// A mounted member cannot be renamed: its declaration lives in a row, not
/// in a workspace file the server can rewrite.
#[test]
fn rename_on_a_mounted_member_is_refused_as_outside_the_workspace() {
    let (db, file) = mounted(CONSUMER);
    for needle in ["describe", "greet", "size"] {
        assert!(
            matches!(
                rename(&db, file, inside(CONSUMER, needle), "renamed"),
                Err(RenameError::NotInWorkspace { .. })
            ),
            "`{needle}` is a row member"
        );
    }
}

/// The runtime-mount lane also installs link stubs under the served root.
/// A served root resolves through its interface even then: a constructor
/// key still names the exported row's field, and goto never lands in a stub.
#[test]
fn a_served_root_with_link_stubs_still_resolves_through_its_rows() {
    let blob = export_blob("app", ROW_LIBRARY);
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("/ide-mounted-stubbed"));
    db.mount("app", blob);
    db.file(
        Path::new("<builtin>/app/runtime_mount_0_0.baml"),
        "class Widget {\n  size int\n}\n",
    );
    let file = db.file(Path::new("/ide-mounted-stubbed/main.baml"), ROW_CONSUMER);
    assert!(matches!(
        symbol_at(&db, file, inside(ROW_CONSUMER, "size: n")),
        Some(SymbolTarget::Field {
            class: DeclRef::External(_),
            field_index: 0,
        })
    ));
    assert!(
        definition_at(&db, file, inside(ROW_CONSUMER, "size)")).is_none(),
        "a stub is never a navigation target"
    );
}

/// Rows the earlier tests never touched: an enum variant, an interface
/// field, and a REQUIRED (bodyless) method.
#[test]
fn mounted_enum_variants_interface_fields_and_required_methods_render_from_rows() {
    let (db, file) = mounted_with(ROW_LIBRARY, MEMBER_CONSUMER);
    let Some(TypeInfo::Symbol {
        declaration, owner, ..
    }) = type_at(&db, file, inside(MEMBER_CONSUMER, "Active"))
    else {
        panic!("a mounted enum variant hovers as a symbol");
    };
    assert_eq!(declaration, "Active: Status");
    assert_eq!(owner.as_deref(), Some("app.Status"));

    let source = "function f(l: app.Labeled) -> int throws never {\n    l.\n}\n";
    let (db, file) = mounted_with(ROW_LIBRARY, source);
    let offset = TextSize::from(u32::try_from(source.find("l.").unwrap() + 2).unwrap());
    let items = completions(&db, file, offset);
    let label = offered(&items, "label");
    assert!(matches!(label.kind, CompletionKind::Field));
    let tag = offered(&items, "tag");
    assert!(matches!(tag.kind, CompletionKind::Method));
    assert!(
        tag.detail
            .as_deref()
            .is_some_and(|d| d.contains("-> string")),
        "a required method completes with its signature: {:?}",
        tag.detail
    );
}
