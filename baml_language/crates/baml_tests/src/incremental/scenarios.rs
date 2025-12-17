//! Incrementality test scenarios.
//!
//! These tests verify that editing BAML files only recomputes the necessary
//! queries, demonstrating Salsa's "early cutoff" optimization.

use baml_db::{SourceFile, baml_hir};
use baml_hir::{function_body, function_signature};
use salsa::Setter;

use super::IncrementalTestDb;

/// Query all function bodies in a file.
/// This is a helper to avoid manually extracting function IDs in tests.
fn query_all_function_bodies(db: &baml_db::RootDatabase, file: SourceFile) {
    let items = baml_hir::file_items(db, file);
    for item in items.items(db) {
        if let baml_hir::ItemId::Function(func_id) = item {
            let _ = function_body(db, *func_id);
        }
    }
}

/// Query all function signatures in a file.
fn query_all_function_signatures(db: &baml_db::RootDatabase, file: SourceFile) {
    let items = baml_hir::file_items(db, file);
    for item in items.items(db) {
        if let baml_hir::ItemId::Function(func_id) = item {
            let _ = function_signature(db, *func_id);
        }
    }
}

/// Test that editing a function body doesn't invalidate the item tree.
///
/// The ItemTree only contains function names, not bodies. So changing a
/// function's prompt should NOT cause file_item_tree to re-execute.
#[test]
fn editing_function_body_preserves_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##,
    );

    // First run - all queries execute
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );

    // Modify only the prompt (body change)
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hi there {{name}}!"#
}
"##
    .to_string());

    // After body change: lex and parse must re-run (different tokens/CST).
    // file_item_tree re-executes because the CST changed, but produces the same
    // ItemTree (body changes don't affect item names/structure).
    // file_items benefits from early cutoff: ItemTree is equal, so it's cached.
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),       // Must re-run: input text changed
            ("parse_result", 1),   // Must re-run: tokens changed
            ("file_item_tree", 1), // Must re-run: CST changed (even though ItemTree will be same)
            ("file_items", 0),     // Early cutoff! ItemTree equal → cached
        ],
    );
}

/// Test that editing one function's body re-executes all signatures.
///
/// Currently all signatures re-execute because they depend on re-parsing the file.
/// This is an area where incrementality could be improved.
#[test]
fn editing_function_body_recomputes_signatures() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Foo(x: string) -> string {
    client GPT4
    prompt #"Foo"#
}

function Bar(y: int) -> int {
    client GPT4
    prompt #"Bar"#
}

function Baz(z: bool) -> bool {
    client GPT4
    prompt #"Baz"#
}
"##,
    );

    // Query all signatures initially - full dependency chain executes
    test_db.assert_executed(
        |db| query_all_function_signatures(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
            ("function_signature", 3),
        ],
    );

    // Modify only Bar's body
    file.set_text(test_db.db_mut()).to(r##"
function Foo(x: string) -> string {
    client GPT4
    prompt #"Foo"#
}

function Bar(y: int) -> int {
    client GPT4
    prompt #"Bar MODIFIED BODY"#
}

function Baz(z: bool) -> bool {
    client GPT4
    prompt #"Baz"#
}
"##
    .to_string());

    // After body-only edit:
    // - lex/parse/file_item_tree must re-run (input changed, CST changed)
    // - file_items is cached (early cutoff: ItemTree equal)
    // - all 3 signatures re-execute (they depend on CST which changed)
    test_db.assert_executed(
        |db| query_all_function_signatures(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 0),         // Early cutoff: ItemTree equal
            ("function_signature", 3), // All re-execute (CST changed)
        ],
    );
}

/// Test that function_body query re-executes when body changes.
#[test]
fn function_body_invalidated_on_body_change() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Test(x: string) -> string {
    client GPT4
    prompt #"Original prompt"#
}
"##,
    );

    // Query body initially - full dependency chain executes
    test_db.assert_executed(
        |db| query_all_function_bodies(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
            ("function_body", 1),
        ],
    );

    // Modify the body
    file.set_text(test_db.db_mut()).to(r##"
function Test(x: string) -> string {
    client GPT4
    prompt #"Modified prompt"#
}
"##
    .to_string());

    // After body change:
    // - lex/parse/file_item_tree must re-run (input changed, CST changed)
    // - file_items is cached (early cutoff: ItemTree equal, same function name)
    // - function_body must re-execute (body content changed)
    test_db.assert_executed(
        |db| query_all_function_bodies(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 0),    // Early cutoff: ItemTree equal
            ("function_body", 1), // Must re-execute: body changed
        ],
    );
}

/// Test that adding a new class invalidates the item tree (as expected).
#[test]
fn adding_class_invalidates_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
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
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
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

    // After adding a class: must re-lex, re-parse, and rebuild item tree
    // (new class means different ItemTree content)
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );
}

/// Test that comment changes have minimal impact.
#[test]
fn comment_changes_recompute_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
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
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );

    // Add a comment
    file.set_text(test_db.db_mut()).to(r#"
// This is a comment
class MyClass {
    field string
}
"#
    .to_string());

    // After comment change: lex and parse must re-run (different tokens/CST).
    // file_item_tree re-executes because the CST changed, but produces the same
    // ItemTree (comments don't affect item names/structure).
    // file_items benefits from early cutoff: ItemTree is equal, so it's cached.
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),       // Must re-run: input text changed
            ("parse_result", 1),   // Must re-run: tokens changed
            ("file_item_tree", 1), // Must re-run: CST changed (even though ItemTree will be same)
            ("file_items", 0),     // Early cutoff! ItemTree equal → cached
        ],
    );
}

/// Test multi-file incrementality - editing one file doesn't affect another.
#[test]
fn editing_one_file_doesnt_affect_other() {
    let mut test_db = IncrementalTestDb::new();

    let file_a = test_db.db_mut().add_file(
        "file_a.baml",
        r#"
class ClassA {
    field string
}
"#,
    );

    let file_b = test_db.db_mut().add_file(
        "file_b.baml",
        r#"
class ClassB {
    value int
}
"#,
    );

    // Query both files initially
    let _ = baml_hir::file_items(test_db.db(), file_a);
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file_b);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );

    // Modify only file_a
    file_a.set_text(test_db.db_mut()).to(r#"
class ClassA {
    field string
    newField int
}
"#
    .to_string());

    // Query file_b - should be fully cached (file_a's change doesn't affect it)
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file_b);
        },
        &[
            ("lex_file", 0),
            ("parse_result", 0),
            ("file_item_tree", 0),
            ("file_items", 0),
        ],
    );

    // Query file_a - should re-execute
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file_a);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );
}

/// Test that renaming a function invalidates the item tree.
#[test]
fn renaming_function_invalidates_item_tree() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function OldName(x: string) -> string {
    client GPT4
    prompt #"test"#
}
"##,
    );

    // Query initially
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );

    // Rename the function
    file.set_text(test_db.db_mut()).to(r##"
function NewName(x: string) -> string {
    client GPT4
    prompt #"test"#
}
"##
    .to_string());

    // After rename: all queries must re-execute (name is part of ItemTree)
    test_db.assert_executed(
        |db| {
            let _ = baml_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_item_tree", 1),
            ("file_items", 1),
        ],
    );
}
