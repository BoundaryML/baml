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

// ─── Phase 3 let-binding tests ────────────────────────────────────────────────

/// Build a synthetic `Item::Let` with an integer literal initializer.
///
/// The produced item has `LetOrigin::Compiler` and an `ExprBody` whose root
/// expression is `Expr::Literal(Literal::Int(value))`.
fn make_let_item(name: &str, value: i64) -> baml_compiler2_ast::Item {
    use baml_compiler2_ast::{AstSourceMap, Expr, ExprBody, Item, LetDef, LetOrigin, Literal};
    use la_arena::Arena;
    use text_size::TextRange;

    let mut exprs: Arena<Expr> = Arena::new();
    let mut source_map = AstSourceMap::new();

    let expr_id = exprs.alloc(Expr::Literal(Literal::Int(value)));
    source_map.expr_spans.alloc(TextRange::default());

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(expr_id),
    };

    Item::Let(LetDef {
        name: baml_base::Name::new(name),
        initializer: Some((body, source_map)),
        origin: LetOrigin::Compiler,
        span: TextRange::default(),
        name_span: TextRange::default(),
    })
}

/// Build a `Item::Let` where the initializer references another let binding by name.
///
/// Used to test dependency detection in `topological_sort_lets`.
fn make_let_item_ref(name: &str, ref_name: &str) -> baml_compiler2_ast::Item {
    use baml_compiler2_ast::{AstSourceMap, Expr, ExprBody, Item, LetDef, LetOrigin};
    use la_arena::Arena;
    use text_size::TextRange;

    let mut exprs: Arena<Expr> = Arena::new();
    let mut source_map = AstSourceMap::new();

    // Expr::Path([ref_name]) — single-segment path referencing another let binding
    let expr_id = exprs.alloc(Expr::Path(vec![baml_base::Name::new(ref_name)]));
    source_map.expr_spans.alloc(TextRange::default());

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(expr_id),
    };

    Item::Let(LetDef {
        name: baml_base::Name::new(name),
        initializer: Some((body, source_map)),
        origin: LetOrigin::Compiler,
        span: TextRange::default(),
        name_span: TextRange::default(),
    })
}

/// Verify that a let binding with a literal initializer:
/// - Gets a global slot allocated (appears in `program.globals`)
/// - Causes `$init` to appear in `program.function_indices`
/// - Causes `$init` to appear in `program.package_init_order`
#[test]
fn let_binding_global_slot_and_init_function() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 0; }");

    // Inject a synthetic let binding `my_const = 42`
    db.set_synthetic_items_for_file(file, vec![make_let_item("my_const", 42)]);

    let program = compile(&db);

    // The let binding should have a global slot allocated in let_global_indices
    let has_let_slot = program
        .let_global_indices
        .keys()
        .any(|k| k.contains("my_const"));
    assert!(
        has_let_slot,
        "expected 'my_const' in let_global_indices, got: {:?}",
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

/// Verify that creating a `BexEngine` with a let binding causes `$init` to run,
/// and that the let-bound value is accessible via a function that references it.
///
/// The test injects `my_const = 42` as a synthetic let binding, then compiles a
/// function `get_const() -> int { my_const }` that references it via `LoadGlobal`.
/// After `BexEngine::new` runs `$init`, calling `get_const()` should return 42.
#[tokio::test]
async fn let_binding_init_runs_at_load_time() {
    use std::sync::Arc;

    use sys_native::SysOpsExt as _;

    let mut db = make_db();
    // The function body uses `my_const` which will resolve to the synthetic let binding.
    let file = db.add_file("test.baml", "function get_const() -> int { my_const }");

    // Inject: my_const = 42
    db.set_synthetic_items_for_file(file, vec![make_let_item("my_const", 42)]);

    let program = generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("compilation should succeed");

    // Verify the let slot starts as Null
    let let_slot = program
        .let_global_indices
        .iter()
        .find(|(k, _)| k.contains("my_const"))
        .map(|(_, &v)| v)
        .expect("expected my_const in let_global_indices");
    assert!(
        matches!(
            program.globals.get(let_slot),
            Some(bex_vm_types::ConstValue::Null)
        ),
        "expected Null initial value for my_const, got: {:?}",
        program.globals.get(let_slot)
    );

    // Create BexEngine — this triggers $init, which evaluates `42` and stores it
    let engine = bex_engine::BexEngine::new(program, Arc::new(sys_types::SysOps::native()), None)
        .expect("BexEngine creation should succeed after $init runs");

    // Call get_const() — should return 42
    let result = engine
        .call_function(
            "user.get_const",
            vec![],
            bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    assert!(result.is_ok(), "call to get_const() failed: {:?}", result);
    let value = result.unwrap();
    assert!(
        matches!(value, bex_engine::BexExternalValue::Int(42)),
        "expected Int(42) from get_const(), got: {:?}",
        value
    );
}

/// Verify that circular let binding dependencies produce a `LoweringError`.
#[test]
fn circular_let_dependencies_produce_error() {
    use baml_compiler2_emit::LoweringError;

    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 0; }");

    // a references b, b references a — circular
    db.set_synthetic_items_for_file(
        file,
        vec![make_let_item_ref("a", "b"), make_let_item_ref("b", "a")],
    );

    let result = generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    );

    assert!(
        matches!(result, Err(LoweringError::Internal(ref msg)) if msg.contains("Circular")),
        "expected circular dependency error, got: {:?}",
        result
    );
}

/// Verify that multiple let bindings with a valid dependency order are sorted correctly.
///
/// `b = a + 0` (references `a`), so `a` must be initialized before `b`.
/// Checks that `$init` exists and both bindings have global slots.
#[test]
fn multiple_let_bindings_with_valid_dependencies() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 0; }");

    // a = 10, b = a (b depends on a)
    db.set_synthetic_items_for_file(
        file,
        vec![
            // Inject b first to ensure topological sort reorders them
            make_let_item_ref("b", "a"),
            make_let_item("a", 10),
        ],
    );

    let program = compile(&db);

    // Both should have global slots in let_global_indices
    let has_a = program
        .let_global_indices
        .keys()
        .any(|k| k.ends_with(".a") || k == "a");
    let has_b = program
        .let_global_indices
        .keys()
        .any(|k| k.ends_with(".b") || k == "b");
    assert!(
        has_a,
        "expected 'a' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );
    assert!(
        has_b,
        "expected 'b' in let_global_indices, got: {:?}",
        program.let_global_indices.keys().collect::<Vec<_>>()
    );

    // $init should be synthesized
    assert!(
        program.function_indices.contains_key("$init"),
        "expected '$init' in function_indices"
    );
}
