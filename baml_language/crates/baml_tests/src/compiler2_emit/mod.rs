//! Integration tests for `baml_compiler2_emit`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs the full
//! compiler2 pipeline through `generate_project_bytecode`, and verifies
//! the resulting `Program` has the expected structure.

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

fn compile(db: &ProjectDatabase) -> bex_vm_types::Program {
    generate_project_bytecode(
        db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("compilation should succeed")
}

#[test]
fn simple_function_compiles() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        "function greet(name: string) -> string { return name; }",
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.greet"),
        "expected 'user.greet' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn builtin_functions_included() {
    let mut db = make_db();
    db.add_file("test.baml", "function f() -> string { return \"x\"; }");
    let program = compile(&db);
    // Builtins from the baml and env packages should be present
    let has_baml = program
        .function_global_indices
        .keys()
        .any(|k| k.starts_with("baml."));
    let has_env = program
        .function_global_indices
        .keys()
        .any(|k| k.starts_with("env."));
    assert!(
        has_baml,
        "expected at least one 'baml.*' function, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_env,
        "expected at least one 'env.*' function, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn enum_variant_lookup() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        r#"
        enum Color { Red Green Blue }
        function pick() -> Color { return Color.Red; }
        "#,
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.pick"),
        "expected 'user.pick' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}

#[test]
fn class_field_lookup() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        r#"
        class Point { x int  y int }
        function origin() -> Point { return Point { x: 0, y: 0 }; }
        "#,
    );
    let program = compile(&db);
    assert!(
        program.function_indices.contains_key("user.origin"),
        "expected 'user.origin' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );
}
