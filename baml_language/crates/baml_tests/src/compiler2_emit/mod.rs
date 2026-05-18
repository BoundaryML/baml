//! Integration tests for `baml_compiler2_emit`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs the full
//! compiler2 pipeline through `generate_project_bytecode`, and verifies
//! the resulting `Program` has the expected structure.

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/compiler2_emit");
const OPTIONAL_DEFAULTS_SOURCE: &str = r#"
function add(base: int, amount: int = base + 2) -> int {
  base + amount
}

function main() -> int {
  add(5)
}
"#;

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

macro_rules! emit_snapshot {
    ($name:expr, $output:expr) => {
        assert_compiler2_snapshot!(SNAPSHOT_PATH, $name, $output);
    };
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

#[test]
fn optional_param_metadata_and_omitted_sentinel_emit() {
    let mut db = make_db();
    db.add_file("test.baml", OPTIONAL_DEFAULTS_SOURCE);
    let program = compile(&db);

    let add_idx = program.function_indices["user.add"];
    let bex_vm_types::Object::Function(add) = &(*program.objects)[add_idx] else {
        panic!("expected user.add to be a function");
    };
    assert_eq!(add.param_has_default, vec![false, true]);
    assert!(
        add.bytecode
            .constants
            .iter()
            .any(|c| matches!(c, bex_vm_types::ConstValue::OmittedArg)),
        "expected default prologue to compare against OmittedArg"
    );

    let main_idx = program.function_indices["user.main"];
    let bex_vm_types::Object::Function(main) = &(*program.objects)[main_idx] else {
        panic!("expected user.main to be a function");
    };
    assert!(
        main.bytecode
            .constants
            .iter()
            .any(|c| matches!(c, bex_vm_types::ConstValue::OmittedArg)),
        "expected omitted source argument to be emitted as OmittedArg"
    );
}

#[test]
fn optional_defaults_emit_snapshot() {
    let mut db = make_db();
    db.add_file("test.baml", OPTIONAL_DEFAULTS_SOURCE);
    let program = compile(&db);
    emit_snapshot!(
        "optional_defaults_emit_snapshot",
        crate::engine::display_user_functions(&program)
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

// ─── Phase 4.6 $init_test chainer tests ───────────────────────────────────────

/// Verify that when a file contains `test` blocks, a root `$init_test` chainer
/// is synthesized in `program.function_indices` with `arity: 1`.
#[test]
fn init_test_chainer_synthesized_when_tests_present() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        r#"
        test "foo" {
          assert.is_true(true)
        }

        test "bar" {
          assert.is_true(true)
        }
        "#,
    );

    let program = compile(&db);

    // The root $init_test chainer should be present in function_indices
    assert!(
        program.function_indices.contains_key("$init_test"),
        "expected '$init_test' in function_indices after test block synthesis, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
    );

    // The chainer should also appear in function_global_indices
    assert!(
        program.function_global_indices.contains_key("$init_test"),
        "expected '$init_test' in function_global_indices, got: {:?}",
        program.function_global_indices.keys().collect::<Vec<_>>()
    );

    // Verify the chainer function has arity 1 (takes the registry parameter)
    let fn_obj_idx = program.function_indices["$init_test"];
    // program.objects derefs to Vec<Object> via Deref, so a plain usize index works.
    let fn_obj = &(*program.objects)[fn_obj_idx];
    let bex_vm_types::Object::Function(chainer) = fn_obj else {
        panic!("expected $init_test to be a Function object, got: {fn_obj:?}");
    };
    assert_eq!(
        chainer.arity, 1,
        "expected $init_test chainer to have arity 1, got: {}",
        chainer.arity
    );
    assert_eq!(chainer.param_names, vec!["registry"]);
    assert_eq!(chainer.param_types.len(), 1);
    assert_eq!(chainer.param_has_default, vec![false]);
}

/// Verify that when a file has NO test blocks, no `$init_test` chainer is synthesized.
#[test]
fn no_init_test_chainer_when_no_tests() {
    let mut db = make_db();
    db.add_file(
        "test.baml",
        "function greet(name: string) -> string { return name; }",
    );

    let program = compile(&db);

    assert!(
        !program.function_indices.contains_key("$init_test"),
        "expected no '$init_test' in function_indices when no test blocks present, got: {:?}",
        program.function_indices.keys().collect::<Vec<_>>()
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
