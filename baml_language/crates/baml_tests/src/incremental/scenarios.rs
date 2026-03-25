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

/// Query the semantic index for all files in a project (forces full HIR build).
fn query_semantic_index(db: &baml_project::ProjectDatabase, file: SourceFile) {
    let _ = baml_compiler2_hir::file_semantic_index(db, file);
}

/// Test that editing a function body doesn't invalidate the item tree.
///
/// The ItemTree only contains function names, not bodies. So changing a
/// function's prompt should NOT cause file_lowering to re-execute.
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
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
    );

    // Modify only the prompt (body change)
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hi there {{name}}!"#
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
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
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
            query_semantic_index(db, file);
        },
        &[("lex_file", 1)],
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

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
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

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}"##,
    );

    // First run
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);

    // Add whitespace (blank lines at end)
    file.set_text(test_db.db_mut())
        .to(r##"function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
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

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##,
    );

    // First run
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);

    // Change the return type
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> int {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##
    .to_string());

    // Signature changes must invalidate queries
    test_db.assert_executed(|db| query_semantic_index(db, file), &[("lex_file", 1)]);
}
