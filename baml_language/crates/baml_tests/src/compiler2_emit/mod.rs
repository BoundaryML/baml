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
    let has_baml_env = program
        .function_global_indices
        .keys()
        .any(|k| k.starts_with("baml.env."));
    assert!(
        has_baml,
        "expected at least one 'baml.*' function, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_baml_env,
        "expected at least one 'baml.env.*' function, got: {:?}",
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

// ─── Phase 3 let-binding tests ────────────────────────────────────────────────
//
// Note: `set_synthetic_items_for_file` was removed from the DB trait as part of
// the compiler2 migration (Phase 2). These tests now use actual BAML source
// declarations (clients, retry_policy) which produce `Item::Let` bindings and
// exercise the same let-binding infrastructure.

/// Verify that a client declaration:
/// - Produces a let binding with a global slot (appears in `let_global_indices`)
/// - Causes `$init` to appear in `program.function_indices`
/// - Causes `$init` to appear in `program.package_init_order`
#[test]
fn let_binding_global_slot_and_init_function() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        r#"
        client<llm> MyClient {
          provider openai
          options { model "gpt-4" }
        }
        function f() -> string { return "x"; }
        "#,
    );

    let program = compile(&db);

    // The client let binding should have a global slot allocated in let_global_indices
    let has_let_slot = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("MyClient"));
    assert!(
        has_let_slot,
        "expected 'MyClient' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    let has_init = program.function_indices.contains_key("$init");
    assert!(
        has_init,
        "expected '$init' in function_indices, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );

    // $init should be in package_init_order
    assert!(
        program.package_init_order.contains(&"$init".to_string()),
        "expected '$init' in package_init_order, got: {:?}",
        program.package_init_order
    );
}

/// Verify that multiple client declarations:
/// - Both get global slots in `let_global_indices`
/// - `$init` is synthesized to initialize them
#[test]
fn multiple_let_bindings_with_valid_dependencies() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        r#"
        client<llm> ClientA {
          provider openai
          options { model "gpt-4" }
        }
        client<llm> ClientB {
          provider openai
          options { model "gpt-3.5-turbo" }
        }
        function f() -> string { return "x"; }
        "#,
    );

    let program = compile(&db);

    // Both clients should have global slots in let_global_indices
    let has_a = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("ClientA"));
    let has_b = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("ClientB"));
    assert!(
        has_a,
        "expected 'ClientA' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_b,
        "expected 'ClientB' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    assert!(
        program.function_indices.contains_key("$init"),
        "expected '$init' in function_indices"
    );
}
