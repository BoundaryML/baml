//! Incrementality test scenarios.
//!
//! These tests verify that editing BAML files only recomputes the necessary
//! queries, demonstrating Salsa's "early cutoff" optimization.

use baml_compiler_hir::{FunctionLoc, function_body, function_signature};
use baml_compiler_tir::function_type_inference;
use baml_db::{SourceFile, baml_compiler_hir};
use salsa::Setter;

use super::IncrementalTestDb;

/// Query all function bodies in a file.
/// This is a helper to avoid manually extracting function IDs in tests.
fn query_all_function_bodies(db: &baml_project::ProjectDatabase, file: SourceFile) {
    let items = baml_compiler_hir::file_items(db, file);
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_id) = item {
            let _ = function_body(db, *func_id);
        }
    }
}

/// Query all function signatures in a file.
fn query_all_function_signatures(db: &baml_project::ProjectDatabase, file: SourceFile) {
    let items = baml_compiler_hir::file_items(db, file);
    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Function(func_id) = item {
            let _sig = function_signature(db, *func_id);
        }
    }
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
    // file_lowering re-executes because the CST changed, but produces the same
    // ItemTree (body changes don't affect item names/structure).
    // file_items benefits from early cutoff: ItemTree is equal, so it's cached.
    test_db.assert_executed(
        |db| {
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),      // Must re-run: input text changed
            ("parse_result", 1),  // Must re-run: tokens changed
            ("file_lowering", 1), // Must re-run: CST changed (even though ItemTree will be same)
            ("file_items", 0),    // Early cutoff! ItemTree equal → cached
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
            ("file_lowering", 1),
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
    // - lex/parse/file_lowering must re-run (input changed, CST changed)
    // - file_items is cached (early cutoff: ItemTree equal)
    // - all 3 signatures re-execute (they depend on CST which changed)
    test_db.assert_executed(
        |db| query_all_function_signatures(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            ("file_lowering", 1),
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
    // - lex/parse/file_lowering must re-run (input changed, CST changed)
    // - file_items is cached (early cutoff: ItemTree equal, same function name)
    // - function_body is cached for LLM functions because the synthetic body
    //   (call_llm_function) only depends on name + params, not the prompt.
    //   Prompt changes are tracked by a separate llm_function_meta query.
    test_db.assert_executed(
        |db| query_all_function_bodies(db, file),
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
            ("file_items", 0),    // Early cutoff: ItemTree equal
            ("function_body", 0), // Cached: LLM synthetic body depends on name+params, not prompt
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
    // file_lowering re-executes because the CST changed, but produces the same
    // ItemTree (comments don't affect item names/structure).
    // file_items benefits from early cutoff: ItemTree is equal, so it's cached.
    test_db.assert_executed(
        |db| {
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),      // Must re-run: input text changed
            ("parse_result", 1),  // Must re-run: tokens changed
            ("file_lowering", 1), // Must re-run: CST changed (even though ItemTree will be same)
            ("file_items", 0),    // Early cutoff! ItemTree equal → cached
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
    let _ = baml_compiler_hir::file_items(test_db.db(), file_a);
    test_db.assert_executed(
        |db| {
            let _ = baml_compiler_hir::file_items(db, file_b);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            let _ = baml_compiler_hir::file_items(db, file_b);
        },
        &[
            ("lex_file", 0),
            ("parse_result", 0),
            ("file_lowering", 0),
            ("file_items", 0),
        ],
    );

    // Query file_a - should re-execute
    test_db.assert_executed(
        |db| {
            let _ = baml_compiler_hir::file_items(db, file_a);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
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
            let _ = baml_compiler_hir::file_items(db, file);
        },
        &[
            ("lex_file", 1),
            ("parse_result", 1),
            ("file_lowering", 1),
            ("file_items", 1),
        ],
    );
}

// ============================================================================
// Type Inference (TIR) Incrementality Tests
// ============================================================================
//
// These tests verify that type inference caching works correctly.
// The key goal of the span→ID migration was to make type inference results
// cacheable even when only whitespace or comments change.

/// Helper to get all function locations from a file.
fn get_function_locs(db: &baml_project::ProjectDatabase, file: SourceFile) -> Vec<FunctionLoc<'_>> {
    let items = baml_compiler_hir::file_items(db, file);
    items
        .items(db)
        .iter()
        .filter_map(|item| {
            if let baml_compiler_hir::ItemId::Function(func_id) = item {
                Some(*func_id)
            } else {
                None
            }
        })
        .collect()
}

/// Query type inference for all functions in a file.
fn query_all_type_inference(db: &baml_project::ProjectDatabase, file: SourceFile) {
    for func in get_function_locs(db, file) {
        let _ = function_type_inference(db, func);
    }
}

/// Test that type inference is cached when nothing changes.
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

    // First run - type inference executes
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Second run without changes - should be fully cached
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 0)],
    );
}

/// Test that whitespace-only changes don't invalidate type inference.
///
/// This is the KEY test for the span→ID migration. Before the migration,
/// whitespace changes would invalidate type inference because spans were
/// stored in the cached InferenceResult. Now that we use position-independent
/// IDs, whitespace changes should NOT cause re-inference.
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

    // First run - type inference executes
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Add whitespace (blank lines at end)
    file.set_text(test_db.db_mut())
        .to(r##"function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}


"##
        .to_string());

    // After whitespace change:
    // - lex/parse must re-run (input changed)
    // - But type inference should be cached (early cutoff)
    //   because the FunctionBody content is semantically identical
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[
            ("lex_file", 1),                // Must re-run
            ("parse_result", 1),            // Must re-run
            ("function_type_inference", 0), // Should be cached!
        ],
    );
}

/// Test that comment-only changes don't invalidate type inference.
///
/// This test verifies that `function_signature` has been split into two queries:
/// - `function_signature` returns position-independent signature data
/// - `function_signature_source_map` returns spans separately
///
/// This enables Salsa early cutoff: when comments shift function positions,
/// `function_signature` returns an equal value, so type inference is cached.
#[test]
fn type_inference_cached_on_comment_change() {
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
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Add a comment before the function (shifts function position)
    file.set_text(test_db.db_mut()).to(r##"
// This function greets the user
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##
    .to_string());

    // Comment changes should NOT invalidate type inference!
    // The function_signature query returns equal values (position-independent),
    // so function_type_inference benefits from Salsa early cutoff.
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[
            ("lex_file", 1),                // Must re-run
            ("parse_result", 1),            // Must re-run
            ("function_type_inference", 0), // Should be cached!
        ],
    );
}

/// Test that changing a function's body DOES invalidate type inference.
#[test]
fn type_inference_invalidated_on_body_change() {
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
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Change the prompt (body change)
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> string {
    client GPT4
    prompt #"Hi there {{name}}!"#
}
"##
    .to_string());

    // Prompt changes invalidate type inference because function_type_inference
    // reads llm_function_meta for Jinja template validation.
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );
}

/// Test that changing a function's signature DOES invalidate type inference.
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
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Change the return type
    file.set_text(test_db.db_mut()).to(r##"
function Greet(name: string) -> int {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##
    .to_string());

    // Signature changes MUST invalidate type inference
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );
}

/// Test that editing one function doesn't invalidate type inference for others.
#[test]
fn type_inference_isolated_between_functions() {
    let mut test_db = IncrementalTestDb::new();

    let file = test_db.db_mut().add_file(
        "test.baml",
        r##"
function Foo(x: string) -> string {
    client GPT4
    prompt #"Foo {{x}}"#
}

function Bar(y: int) -> int {
    client GPT4
    prompt #"Bar {{y}}"#
}
"##,
    );

    // First run - both functions get type-checked
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 2)],
    );

    // Modify only Foo's body
    file.set_text(test_db.db_mut()).to(r##"
function Foo(x: string) -> string {
    client GPT4
    prompt #"Modified Foo {{x}}"#
}

function Bar(y: int) -> int {
    client GPT4
    prompt #"Bar {{y}}"#
}
"##
    .to_string());

    // Only Foo's prompt changed, so only Foo's type inference re-runs.
    // Bar's llm_function_meta is unchanged, so Bar's type inference is cached.
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );
}

/// Test that adding a new class doesn't invalidate existing function type inference.
#[test]
fn type_inference_stable_when_adding_unrelated_class() {
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
    test_db.assert_executed(
        |db| query_all_type_inference(db, file),
        &[("function_type_inference", 1)],
    );

    // Add an unrelated class
    file.set_text(test_db.db_mut()).to(r##"
class Person {
    name string
}

function Greet(name: string) -> string {
    client GPT4
    prompt #"Hello {{name}}"#
}
"##
    .to_string());

    // Adding a class changes the project-level type context, which may
    // invalidate type inference. This test documents current behavior.
    // Ideally we'd achieve early cutoff here too.
    let (_, executed) = test_db.log_executed(|db| query_all_type_inference(db, file));
    let inference_count = executed
        .iter()
        .filter(|s| s.contains("function_type_inference"))
        .count();

    // Document behavior - this may be 0 (ideal) or 1 (acceptable)
    println!(
        "Type inference re-executions after adding unrelated class: {}",
        inference_count
    );
}
