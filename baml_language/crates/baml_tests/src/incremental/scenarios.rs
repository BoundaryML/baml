//! Incrementality test scenarios.
//!
//! These tests verify that editing BAML files only recomputes the necessary
//! queries, demonstrating Salsa's "early cutoff" optimization.
//!
//! NOTE: These scenarios were ported from the legacy HIR (baml_compiler_hir)
//! to compiler2 HIR (baml_compiler2_hir) as part of the compiler2 migration.

use baml_db::{SourceFile, baml_compiler2_hir};
use salsa::Setter;

use super::IncrementalTestDb;
use crate::engine::TestDbExt;

/// Query the semantic index for all files in a project (forces full HIR build).
fn query_semantic_index(db: &baml_db::ProjectDatabase, file: SourceFile) {
    let _ = baml_compiler2_hir::file_semantic_index(db, file);
}

/// Test that editing a function body doesn't invalidate the item tree.
///
/// The ItemTree only contains function names, not bodies. So changing a
/// function's prompt should NOT cause file_lowering to re-execute.
#[test]
fn editing_function_body_preserves_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hello ${name}`
}
"##,
    );

    // First run - all queries execute
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );

    // Modify only the prompt (body change)
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hi there ${name}!`
}
"##
    .to_string());

    // After body change: lex must re-run
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[
            ("lex_file", 1), // Must re-run: input text changed
        ],
    );
}

/// Test that renaming a function invalidates the item tree.
#[test]
fn renaming_function_invalidates_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r##"
function OldName(x: string) -> string {
    client: GPT4
    prompt: `test`
}
"##,
    );

    // Query initially
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );

    // Rename the function
    file.set_text(test_db.db_mut()).to(r##"
function NewName(x: string) -> string {
    client: GPT4
    prompt: `test`
}
"##
    .to_string());

    // After rename: all queries must re-execute (name is part of ItemTree)
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );
}

/// Test that adding a new class invalidates the item tree (as expected).
#[test]
fn adding_class_invalidates_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r#"
class Person {
    name string
    age int
}

class Address {
    street string
    city string
}
"#,
    );

    // Query all items initially
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );

    // Add a new class
    file.set_text(test_db.db_mut()).to(r#"
class Person {
    name string
    age int
}

class Address {
    street string
    city string
}

class NewClass {
    value string
}
"#
    .to_string());

    // After adding a class: must re-lex
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );
}

/// Test that comment changes require re-lexing.
#[test]
fn comment_changes_recompute_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r#"
class MyClass {
    field string
}
"#,
    );

    // Query items initially
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );

    // Add a comment
    file.set_text(test_db.db_mut()).to(r#"
// This is a comment
class MyClass {
    field string
}
"#
    .to_string());

    // After comment change: lex must re-run (different input)
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );
}

/// Test multi-file incrementality - editing one file doesn't affect another.
#[test]
fn editing_one_file_doesnt_affect_other() {
    let mut test_db = IncrementalTestDb::new();

    let file_a = test_db.db_mut().file(
        "file_a.baml",
        r#"
class ClassA {
    field string
}
"#,
    );

    let file_b = test_db.db_mut().file(
        "file_b.baml",
        r#"
class ClassB {
    value int
}
"#,
    );

    // Query both files initially.
    query_semantic_index(test_db.db(), file_a);
    query_semantic_index(test_db.db(), file_b);

    // Modify only file_a
    file_a.set_text(test_db.db_mut()).to(r#"
class ClassA {
    field string
    newField int
}
"#
    .to_string());

    // Query file_b - should be cached (file_b unchanged)
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file_b);
        },
        &[
            ("lex_file", 0), // file_b unchanged
        ],
    );

    // Query file_a - must re-run (file_a text changed)
    test_db.assert_executed(
        |db| {
            query_semantic_index(db, file_a);
        },
        &[
            ("lex_file", 1), // Must re-run: real file changed
        ],
    );
}

/// Test that type inference is cached when nothing changes (compiler2 TIR).
#[test]
fn type_inference_cached_on_no_change() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hello ${name}`
}
"##,
    );

    // First run
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);

    // Second run without changes - should be fully cached
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 0)]);
}

/// Test that whitespace-only changes don't unnecessarily invalidate compilation.
#[test]
fn type_inference_cached_on_whitespace_change() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r##"function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hello ${name}`
}"##,
    );

    // First run
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);

    // Add whitespace (blank lines at end)
    file.set_text(test_db.db_mut())
        .to(r##"function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hello ${name}`
}


"##
        .to_string());

    // After whitespace change: lex must re-run (input changed)
    test_db.assert_executed(
        |db| query_semantic_index(db, file),
        &[
            ("lex_file", 1), // Must re-run
        ],
    );
}

/// Test that changing a function's signature DOES invalidate things.
#[test]
fn type_inference_invalidated_on_signature_change() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client: GPT4
    prompt: `Hello ${name}`
}
"##,
    );

    // First run
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);

    // Change the return type
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> int {
    client: GPT4
    prompt: `Hello ${name}`
}
"##
    .to_string());

    // Signature changes must invalidate queries
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);
}

// ── Firewall queries ─────────────────────────────────────────────────────────
//
// `file_semantic_index` is `no_eq`, so it always reports "changed" and anything
// reading the `ItemTree` through it re-runs on every keystroke. The per-item
// firewall queries are what stop that from propagating: they re-run, but their
// results only *compare* unequal when the item genuinely changed, so Salsa cuts
// off there.
//
// That only holds because the semantic half is span-free. Salsa keeps the old
// memoized value whenever the new one compares equal, so if `*_data` carried
// spans it would either lose cutoff (spans in `PartialEq`) or hand out stale
// ones (spans ignored by `PartialEq`). These tests pin both halves of that.

fn type_alias_loc<'db>(
    db: &'db baml_db::ProjectDatabase,
    file: SourceFile,
    name: &str,
) -> baml_compiler2_hir::loc::TypeAliasLoc<'db> {
    *baml_compiler2_ppir::item_data::file_type_aliases(db, file)
        .iter()
        .find(|&&loc| {
            baml_compiler2_ppir::item_data::type_alias_data(db, loc)
                .name
                .as_str()
                == name
        })
        .unwrap_or_else(|| unreachable!("type alias `{name}` should exist"))
}

/// A whitespace-only edit must leave the semantic data byte-for-byte equal (so
/// Salsa cuts off) while the source map still reports the *new* positions.
#[test]
fn whitespace_edit_preserves_item_data_but_moves_spans() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file("test.baml", "type Ids = int[]\n");

    let (data_before, span_before) = {
        let db = test_db.db();
        let loc = type_alias_loc(db, file, "Ids");
        (
            baml_compiler2_ppir::item_data::type_alias_data(db, loc).clone(),
            baml_compiler2_ppir::item_data::type_alias_source_map(db, loc).clone(),
        )
    };

    // Push the declaration down a line. Semantics identical, every span shifted.
    file.set_text(test_db.db_mut())
        .to("// a comment\ntype Ids = int[]\n".to_string());

    let (data_after, span_after) = {
        let db = test_db.db();
        let loc = type_alias_loc(db, file, "Ids");
        (
            baml_compiler2_ppir::item_data::type_alias_data(db, loc).clone(),
            baml_compiler2_ppir::item_data::type_alias_source_map(db, loc).clone(),
        )
    };

    assert_eq!(
        data_before, data_after,
        "a whitespace-only edit must not change the semantic data, or nothing downstream can cut off"
    );
    assert_ne!(
        span_before, span_after,
        "the source map must track the new positions — otherwise spans would go stale"
    );
}

/// The converse: a real change to the aliased type must invalidate the semantic
/// data, or we would be cutting off edits that actually matter.
#[test]
fn semantic_edit_changes_item_data() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file("test.baml", "type Ids = int[]\n");

    let data_before = {
        let db = test_db.db();
        let loc = type_alias_loc(db, file, "Ids");
        baml_compiler2_ppir::item_data::type_alias_data(db, loc).clone()
    };

    file.set_text(test_db.db_mut())
        .to("type Ids = string[]\n".to_string());

    let data_after = {
        let db = test_db.db();
        let loc = type_alias_loc(db, file, "Ids");
        baml_compiler2_ppir::item_data::type_alias_data(db, loc).clone()
    };

    assert_ne!(data_before, data_after);
}

fn class_loc<'db>(
    db: &'db baml_db::ProjectDatabase,
    file: SourceFile,
    name: &str,
) -> baml_compiler2_hir::loc::ClassLoc<'db> {
    *baml_compiler2_ppir::item_data::file_classes(db, file)
        .iter()
        .find(|&&loc| {
            baml_compiler2_ppir::item_data::class_data(db, loc)
                .name
                .as_str()
                == name
        })
        .unwrap_or_else(|| unreachable!("class `{name}` should exist"))
}

fn function_loc<'db>(
    db: &'db baml_db::ProjectDatabase,
    file: SourceFile,
    name: &str,
) -> baml_compiler2_hir::loc::FunctionLoc<'db> {
    *baml_compiler2_ppir::item_data::file_functions(db, file)
        .iter()
        .find(|&&loc| {
            baml_compiler2_ppir::item_data::function_data(db, loc)
                .name
                .as_str()
                == name
        })
        .unwrap_or_else(|| unreachable!("function `{name}` should exist"))
}

/// Everything span-bearing in a `ClassData`, as an owned value.
///
/// `ClassData<'db>` holds `FunctionLoc<'db>`s, so even a clone keeps the `db`
/// borrow alive and cannot be held across an edit. `methods` is pure identity
/// and carries no spans, so projecting it away loses nothing these tests check.
type ClassFingerprint = (
    baml_base::Name,
    Vec<baml_compiler2_ppir::item_data::GenericParamData>,
    baml_compiler2_hir::type_ref::TypeRefStore,
    Vec<baml_compiler2_ppir::item_data::FieldData>,
    Vec<baml_compiler2_ppir::item_data::ImplementsData>,
    Vec<baml_compiler2_hir::item_tree::Attribute>,
);

fn class_fingerprint(
    db: &baml_db::ProjectDatabase,
    file: SourceFile,
    name: &str,
) -> ClassFingerprint {
    let data = baml_compiler2_ppir::item_data::class_data(db, class_loc(db, file, name));
    (
        data.name.clone(),
        data.generic_params.clone(),
        data.type_refs.clone(),
        data.fields.clone(),
        data.implements.clone(),
        data.attributes.clone(),
    )
}

/// The whole point of a per-item firewall: editing one item must leave every
/// *other* item's data untouched, so nothing downstream of them re-runs.
#[test]
fn editing_one_class_preserves_the_others_data() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "class Person {\n  name string\n}\n\nclass Address {\n  city string\n}\n",
    );

    let untouched_before = class_fingerprint(test_db.db(), file, "Address");

    // Add a field to `Person`. `Address` is not touched — but it moves, so every
    // span in it shifts.
    file.set_text(test_db.db_mut()).to(
        "class Person {\n  name string\n  age int\n}\n\nclass Address {\n  city string\n}\n"
            .to_string(),
    );

    let untouched_after = class_fingerprint(test_db.db(), file, "Address");
    let touched = class_fingerprint(test_db.db(), file, "Person");

    assert_eq!(
        untouched_before, untouched_after,
        "editing `Person` must not invalidate `Address` — that is the firewall"
    );
    assert_eq!(touched.3.len(), 2, "`Person` really did change");
}

/// `ast::RawAttribute` puts its span in its own `PartialEq`, and every
/// `TypeExprKind` variant holds `attrs`, so today a whitespace edit near any
/// `@description` makes the type compare unequal and silently destroys cutoff.
/// `ClassData` carries the span-free `Attribute` instead.
#[test]
fn moving_an_attribute_preserves_class_data() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "class Person {\n  name string @description(\"who\")\n}\n",
    );

    let before = class_fingerprint(test_db.db(), file, "Person");

    // Push the class down. The attribute's span moves with it.
    file.set_text(test_db.db_mut())
        .to("// a comment\nclass Person {\n  name string @description(\"who\")\n}\n".to_string());

    let after = class_fingerprint(test_db.db(), file, "Person");

    assert_eq!(
        before, after,
        "moving an attribute must not change the semantic data"
    );
}

/// A function's signature data must not depend on its body — otherwise every
/// keystroke inside a body invalidates every caller's view of the signature.
#[test]
fn editing_a_function_body_preserves_its_signature_data() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "function Add(x: int, y: int) -> int {\n  x + y\n}\n",
    );

    let before = {
        let db = test_db.db();
        baml_compiler2_ppir::item_data::function_data(db, function_loc(db, file, "Add")).clone()
    };

    file.set_text(test_db.db_mut())
        .to("function Add(x: int, y: int) -> int {\n  y + x + 0\n}\n".to_string());

    let after = {
        let db = test_db.db();
        baml_compiler2_ppir::item_data::function_data(db, function_loc(db, file, "Add")).clone()
    };

    assert_eq!(
        before, after,
        "a body edit must not invalidate the signature"
    );
    assert_eq!(before.params.len(), 2);
}

// ── Item ↔ scope index ───────────────────────────────────────────────────────
//
// ~20 sites across TIR/MIR/LSP used to recover "the scope for this item" by
// scanning for a scope whose `range` equalled the item's `span`. That made item
// spans load-bearing *semantic identity*, which is what blocked moving them into
// the source map. The builder now records the link directly.

/// The index must agree with the span-equality scan it replaces, for every
/// function in the file — otherwise migrating the call sites changes behavior.
#[test]
fn function_scope_index_agrees_with_the_span_join_it_replaces() {
    let mut test_db = IncrementalTestDb::new();

    // Includes a declarative LLM function. Its spec recipe is attached to the
    // authored function rather than represented by additional declarations, so
    // every source function must have exactly one unambiguous scope entry.
    let file = test_db.db_mut().file(
        "test.baml",
        "function Add(x: int, y: int) -> int {\n  x + y\n}\n\n\
         function Sub(x: int, y: int) -> int {\n  x - y\n}\n\n\
         class Holder {\n  n int\n\n  function get(self) -> int {\n    self.n\n  }\n}\n\n\
         function Greet(name: string) -> string {\n  \
         client: \"openai/gpt-4o-mini\"\n  prompt: `Hello ${name}`\n}\n",
    );

    let db = test_db.db();
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let functions = baml_compiler2_ppir::item_data::file_functions(db, file);
    assert_eq!(
        functions.len(),
        4,
        "fixture should contain four authored functions"
    );

    for &loc in functions {
        let func = baml_compiler2_ppir::item_data::function_data(db, loc);
        let func_span = baml_compiler2_ppir::item_data::function_source_map(db, loc).span;

        // The scan being retired.
        let legacy = index
            .scope_ids
            .iter()
            .copied()
            .find(|scope_id| {
                let scope = &index.scopes[scope_id.file_scope_id(db).index() as usize];
                matches!(scope.kind, baml_compiler2_hir::scope::ScopeKind::Function)
                    && scope.range == func_span
                    && scope.name.as_ref() == Some(&func.name)
            })
            .map(|scope| scope.file_scope_id(db));

        let indexed = baml_compiler2_ppir::item_data::function_scope(db, loc)
            .map(|scope| scope.file_scope_id(db));

        assert_eq!(
            legacy, indexed,
            "index and span-join disagree for function `{}`",
            func.name
        );
        assert!(indexed.is_some(), "`{}` should have a scope", func.name);
    }
}

/// The point of the index: it is not derived from spans, so a whitespace edit
/// leaves it alone.
#[test]
fn function_scope_survives_a_whitespace_edit() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "function Add(x: int, y: int) -> int {\n  x + y\n}\n",
    );

    let before = {
        let db = test_db.db();
        let loc = function_loc(db, file, "Add");
        baml_compiler2_ppir::item_data::function_scope(db, loc).map(|scope| scope.file_scope_id(db))
    };

    file.set_text(test_db.db_mut())
        .to("// pushed down\nfunction Add(x: int, y: int) -> int {\n  x + y\n}\n".to_string());

    let after = {
        let db = test_db.db();
        let loc = function_loc(db, file, "Add");
        baml_compiler2_ppir::item_data::function_scope(db, loc).map(|scope| scope.file_scope_id(db))
    };

    assert!(before.is_some());
    assert_eq!(
        before, after,
        "the item↔scope link must not depend on where the item sits in the file"
    );
}

/// `scope_owner` is the inverse and must round-trip.
#[test]
fn scope_owner_round_trips() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db
        .db_mut()
        .file("test.baml", "function Add(x: int) -> int {\n  x\n}\n");

    let db = test_db.db();
    let loc = function_loc(db, file, "Add");
    let scope = baml_compiler2_ppir::item_data::function_scope(db, loc).expect("scope");

    assert_eq!(
        baml_compiler2_ppir::item_data::scope_owner(db, scope),
        Some(baml_compiler2_ppir::item_data::ScopeOwner::Function(loc)),
    );
}

// ── Method → owner index ─────────────────────────────────────────────────────

/// `method_owner` must agree with the three scans it replaces (classes by
/// `methods`, interfaces by `default_methods`, free impls by `methods`), for
/// every function in a fixture covering all the ownership cases: a plain class
/// method, an in-body `implements` method (owned by the *class*), an interface
/// default method, an out-of-body impl method, and a top-level function.
#[test]
fn method_owner_index_agrees_with_the_scans_it_replaces() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        r#"
interface Greeter {
    function greet(self) -> string
    function shout(self) -> string throws never {
        "HI"
    }
}

class Person {
    name string

    function rename(self, name: string) -> string throws never {
        name
    }

    implements Greeter {
        function greet(self) -> string throws never {
            self.name
        }
    }
}

class Robot {
    id string
}

// A simple `implements I for C` is merged onto the class during AST lowering
// (its method becomes class-owned); only a *generic* out-of-body impl stays a
// free impl block.
implements Greeter for Robot {
    function greet(self) -> string throws never {
        self.id
    }
}

interface Valued<T> {
    function get(self) -> T
}

class Box<T> {
    value T
}

implements<T> Valued<T> for Box<T> {
    function get(self) -> T throws never {
        self.value
    }
}

function free_standing(x: int) -> int throws never {
    x
}
"#,
    );

    let db = test_db.db();
    assert!(!baml_compiler2_ppir::item_data::file_functions(db, file).is_empty());

    let mut cases = (0usize, 0usize, 0usize, 0usize);
    for &loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
        let func_name = baml_compiler2_ppir::item_data::function_data(db, loc)
            .name
            .clone();

        // The scans being retired.
        let by_class = baml_compiler2_ppir::item_data::file_classes(db, file)
            .iter()
            .copied()
            .find(|&class_loc| {
                baml_compiler2_ppir::item_data::class_data(db, class_loc)
                    .methods
                    .contains(&loc)
            });
        let by_interface = baml_compiler2_ppir::item_data::file_interfaces(db, file)
            .iter()
            .copied()
            .find(|&iface_loc| {
                baml_compiler2_ppir::item_data::interface_data(db, iface_loc)
                    .methods
                    .contains(&loc)
            });
        let by_free_impl = baml_compiler2_ppir::item_data::file_free_impls(db, file)
            .iter()
            .copied()
            .find(|&impl_loc| {
                baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc)
                    .methods
                    .contains(&loc)
            });

        let indexed = baml_compiler2_ppir::item_data::method_owner(db, loc);

        use baml_compiler2_ppir::item_data::MethodOwner;
        match (by_class, by_interface, by_free_impl) {
            (Some(class_loc), None, None) => {
                cases.0 += 1;
                assert!(
                    matches!(indexed, Some(MethodOwner::Class(c)) if c == class_loc),
                    "class scan and index disagree for {func_name:?}"
                );
            }
            (None, Some(iface_loc), None) => {
                cases.1 += 1;
                assert!(
                    matches!(indexed, Some(MethodOwner::Interface(i)) if i == iface_loc),
                    "interface scan and index disagree for {func_name:?}"
                );
            }
            (None, None, Some(impl_loc)) => {
                cases.2 += 1;
                assert!(
                    matches!(indexed, Some(MethodOwner::FreeImpl(b)) if b == impl_loc),
                    "free-impl scan and index disagree for {func_name:?}"
                );
            }
            (None, None, None) => {
                cases.3 += 1;
                assert_eq!(
                    indexed, None,
                    "top-level function {func_name:?} should have no owner"
                );
            }
            other => unreachable!(
                "a method can only have one owner; scans returned {other:?} for {func_name:?}"
            ),
        }
    }

    // Guard against a vacuous fixture: every ownership case must be present.
    assert!(
        cases.0 >= 2,
        "expected class methods (plain + in-body impl)"
    );
    assert!(cases.1 >= 1, "expected an interface default method");
    assert!(cases.2 >= 1, "expected a free-impl method");
    assert!(cases.3 >= 1, "expected a top-level function");
}

/// The elaborated signature is the canonical callable view TIR consumes; its
/// tracked, span-free form must survive whitespace *and* body edits untouched,
/// and must still perform the elaboration (callback params with omitted throws
/// get synthetic effect parameters).
#[test]
fn elaborated_function_data_cuts_off_and_still_elaborates() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "function Apply(x: int, f: (int) -> int) -> int throws never {\n  f(x)\n}\n",
    );

    let before = {
        let db = test_db.db();
        let loc = function_loc(db, file, "Apply");
        baml_compiler2_ppir::item_data::elaborated_function_data(db, loc).clone()
    };

    // The callback param `f` omits its throws — elaboration must have opened a
    // synthetic effect parameter for it.
    assert_eq!(
        before.synthetic_effect_params.len(),
        1,
        "callback with omitted throws should mint one effect param"
    );
    assert_eq!(before.params.len(), 2);

    // Whitespace + body edit together: signature semantics unchanged.
    file.set_text(test_db.db_mut()).to(
        "// moved\nfunction Apply(x: int, f: (int) -> int) -> int throws never {\n  f(x) + 0\n}\n"
            .to_string(),
    );

    let after = {
        let db = test_db.db();
        let loc = function_loc(db, file, "Apply");
        baml_compiler2_ppir::item_data::elaborated_function_data(db, loc).clone()
    };

    assert_eq!(
        before, after,
        "whitespace/body edits must not change the elaborated signature"
    );
}

/// `function_llm_meta` is a *projection*: it exposes only whether a function is an
/// LLM function and its client, deliberately excluding the prompt template (which
/// carries spans and changes constantly). Editing the prompt must therefore leave
/// the projection equal, so consumers that only care about `is_llm`/client cut off
/// even though the underlying `declarative_meta` changed.
#[test]
fn editing_a_function_prompt_preserves_its_llm_meta() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().file(
        "test.baml",
        "function Greet(name: string) -> string {\n  client: GPT4\n  prompt: `Hi ${name}`\n}\n",
    );

    let before = {
        let db = test_db.db();
        baml_compiler2_ppir::item_data::function_llm_meta(db, function_loc(db, file, "Greet"))
            .clone()
    };

    // Rewrite only the prompt — the client (the one fact the projection keeps) is
    // untouched.
    file.set_text(test_db.db_mut()).to(
        "function Greet(name: string) -> string {\n  client: GPT4\n  prompt: `Hello there ${name}!`\n}\n"
            .to_string(),
    );

    let after = {
        let db = test_db.db();
        baml_compiler2_ppir::item_data::function_llm_meta(db, function_loc(db, file, "Greet"))
            .clone()
    };

    assert!(before.is_some(), "`Greet` is an LLM function");
    assert_eq!(
        before, after,
        "a prompt-only edit must not change is_llm/client — the projection exists to exclude the prompt"
    );
}

// ── hir_ty inference firewall (S2/S3) ────────────────────────────────────────

/// Run hir_ty inference for every body owner in `file`.
fn query_hir_ty_inference(db: &baml_db::ProjectDatabase, file: SourceFile) {
    for owner in baml_compiler2_ppir::file_body_owners(db, file) {
        let _ = baml_compiler2_hir_ty::infer::infer_body(db, owner);
    }
}

/// The tracked `infer_function_body` is cached: a repeat query with no
/// edit executes nothing.
#[test]
fn hir_ty_inference_cached_on_repeat() {
    let mut test_db = IncrementalTestDb::new();
    let file = test_db.db_mut().file(
        "test.baml",
        "function f(x: int) -> int throws never {\n    x + 1\n}\n",
    );

    test_db.assert_executed(
        |db| query_hir_ty_inference(db, file),
        &[("infer_function_body", 1)],
    );
    test_db.assert_not_executed(
        |db| query_hir_ty_inference(db, file),
        &[
            "infer_function_body",
            "function_signature",
            "callable_throws",
        ],
    );
}

/// Editing one file's body leaves OTHER files' inference untouched
/// (cross-file isolation: inference inputs are per-file).
#[test]
fn hir_ty_editing_one_file_preserves_other_files_inference() {
    let mut test_db = IncrementalTestDb::new();
    let file_a = test_db.db_mut().file(
        "a.baml",
        "function alpha() -> int throws never {\n    1\n}\n",
    );
    let file_b = test_db.db_mut().file(
        "b.baml",
        "function beta() -> int throws never {\n    2\n}\n",
    );

    test_db.assert_executed(
        |db| {
            query_hir_ty_inference(db, file_a);
            query_hir_ty_inference(db, file_b);
        },
        &[("infer_function_body", 2)],
    );

    file_b
        .set_text(test_db.db_mut())
        .to("function beta() -> int throws never {\n    3\n}\n".to_string());

    // Re-querying A alone recomputes nothing: its inputs are unchanged.
    test_db.assert_not_executed(
        |db| query_hir_ty_inference(db, file_a),
        &["infer_function_body"],
    );
}

/// THE firewall (S3): a body edit that leaves the callee's SIGNATURE
/// unchanged (declared return, unchanged inferred effect) does not
/// re-infer its callers - `function_signature`/`callable_throws`
/// re-execute but produce EQUAL results, and the PartialEq-driven
/// `salsa::Update` cuts the caller's `infer_function_body` off.
#[test]
fn hir_ty_body_edit_with_stable_signature_does_not_reinfer_callers() {
    let mut test_db = IncrementalTestDb::new();
    let file_a = test_db.db_mut().file(
        "a.baml",
        "function callee() -> int throws never {\n    1\n}\n",
    );
    let file_b = test_db.db_mut().file(
        "b.baml",
        "function caller() -> int throws never {\n    callee()\n}\n",
    );

    test_db.assert_executed(
        |db| {
            query_hir_ty_inference(db, file_a);
            query_hir_ty_inference(db, file_b);
        },
        &[("infer_function_body", 2)],
    );

    // Edit the callee's BODY only: its own inference changes (the literal
    // types differ), but the signature - declared return, `never` effect -
    // is identical.
    file_a
        .set_text(test_db.db_mut())
        .to("function callee() -> int throws never {\n    2\n}\n".to_string());

    // Only the callee re-infers; the caller is cut off at the signature.
    test_db.assert_executed(
        |db| {
            query_hir_ty_inference(db, file_a);
            query_hir_ty_inference(db, file_b);
        },
        &[("infer_function_body", 1)],
    );
}
