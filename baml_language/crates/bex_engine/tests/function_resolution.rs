//! Tests for the canonical function-name resolver shared by `baml run`,
//! `baml pack`, the engine call layer, and the sysop LLM/`$new` lookups.
//!
//! The unit tests in `sys_types::tests` cover `resolve_name` mechanics over
//! a raw `HashMap`. These tests cover the wire-up through a compiled
//! engine: namespaced functions resolve via suffix scan, ambiguous bare
//! names surface as errors instead of silent first-match, and stdlib
//! functions are never reachable through `find_user_function`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::sync::Arc;

use baml_db::testing::compile_multi_file;
use bex_engine::{BexEngine, CallId, EngineError, FunctionCallContextBuilder};
use bex_heap::BexExternalValue;
use sys_native::SysOpsExt;

fn engine(files: &[(&str, &str)]) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_multi_file(files),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("BexEngine::new should succeed"),
    )
}

/// `find_user_function` resolves a namespaced function via the shared
/// `resolve_name` suffix-scan rule, the new capability introduced when
/// CLI surfaces were migrated onto the canonical resolver.
#[test]
fn find_user_function_resolves_namespaced_via_suffix_scan() {
    let eng = engine(&[("ns_lorem/foo.baml", "function Bar() -> int { 42 }")]);

    let info = eng
        .find_user_function("Bar")
        .expect("bare-name suffix scan should resolve user.lorem.Bar");
    assert_eq!(info.qualified_name, "user.lorem.Bar");
    assert_eq!(info.display_name, "lorem.Bar");

    let info_qualified = eng
        .find_user_function("user.lorem.Bar")
        .expect("qualified name should resolve");
    assert_eq!(info_qualified.qualified_name, "user.lorem.Bar");

    let info_display = eng
        .find_user_function("lorem.Bar")
        .expect("display-name lookup should resolve via suffix scan");
    assert_eq!(info_display.qualified_name, "user.lorem.Bar");
}

/// Bare name matching two namespaces is ambiguous; the engine call layer
/// surfaces `FunctionNotFound` rather than silently picking one match.
/// The `ResolveOutcome::Ambiguous` arm in `BexEngine::lookup_function`'s
/// `.found()` call is what enforces this.
#[tokio::test]
async fn call_function_ambiguous_bare_name_errors() {
    let eng = engine(&[
        ("ns_a/x.baml", "function Func() -> int { 1 }"),
        ("ns_b/x.baml", "function Func() -> int { 2 }"),
    ]);

    let result = eng
        .call_function(
            "Func",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(EngineError::FunctionNotFound { name }) => assert_eq!(name, "Func"),
        other => panic!("expected FunctionNotFound, got {other:?}"),
    }

    // Qualified names still resolve unambiguously.
    let ok = eng
        .call_function(
            "user.a.Func",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;
    assert!(ok.is_ok(), "qualified name should still resolve: {ok:?}");
}

/// `find_user_function` post-filters to user-callable bytecode, so the
/// stdlib's `baml.json.serialize` (and friends) must never be reachable
/// via either bare-name suffix scan or fully qualified lookup. A
/// regression here would let `baml run serialize` hijack `baml.json.serialize`
/// with arbitrary arguments.
#[test]
fn find_user_function_does_not_expose_stdlib() {
    let eng = engine(&[("main.baml", "function main() -> int { 1 }")]);

    assert!(
        eng.find_user_function("serialize").is_none(),
        "bare suffix scan must not match stdlib baml.json.serialize"
    );
    assert!(
        eng.find_user_function("baml.json.serialize").is_none(),
        "qualified stdlib name must not be reachable via find_user_function"
    );
    assert!(
        eng.find_user_function("baml.sys.exit").is_none(),
        "stdlib sys functions must not be reachable via find_user_function"
    );

    // Sanity: the actual user function is still reachable both ways,
    // proving the filter rejects stdlib without rejecting user code.
    let main_info = eng
        .find_user_function("main")
        .expect("user.main should resolve via display name");
    assert_eq!(main_info.qualified_name, "user.main");
}

/// `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` →
/// `FunctionKind::SysOp`. Calling it as an entry point should yield to the
/// engine, resume the synthesized entry frame, and return the sysop result.
#[tokio::test]
async fn sysop_fs_exists_callable_as_entry_point() {
    let eng = engine(&[("main.baml", "function main() -> int { 1 }")]);

    let result = eng
        .call_function(
            "baml.fs.exists",
            vec![BexExternalValue::String(".".into())],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Bool(exists)) => assert!(exists, "'.' should exist"),
        other => panic!("expected Ok(Bool(true)) from baml.fs.exists as entry, got {other:?}"),
    }

    // Sanity: bytecode entries still work.
    let ok = eng
        .call_function(
            "user.main",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;
    assert!(ok.is_ok(), "bytecode entry must still resolve: {ok:?}");
}

/// `baml.sys.argv() -> string[]` is a `$rust_function` → `FunctionKind::Native`.
/// Calling it as an entry point should run the native and return the argument
/// array, not reject with `NotInvokableAsEntry`. The engine here is built
/// without host argv, so the array is legitimately empty — the shape is what
/// this asserts.
#[tokio::test]
async fn native_argv_callable_as_entry_point() {
    let eng = engine(&[("main.baml", "function main() -> int { 1 }")]);

    let result = eng
        .call_function(
            "baml.sys.argv",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Array { .. }) => {}
        other => panic!("expected Ok(Array {{ .. }}) from baml.sys.argv as entry, got {other:?}"),
    }
}

/// Generic `$rust_function` entries must still expose host-provided type args
/// to the native through `current_call_type_args()`.
#[tokio::test]
async fn generic_native_json_to_string_callable_as_entry_point() {
    let eng = engine(&[("main.baml", "function main() -> int { 1 }")]);

    let result = eng
        .call_function(
            "baml.json.to_string",
            vec![BexExternalValue::Int(7)],
            FunctionCallContextBuilder::new(CallId::next())
                .with_type_args(indexmap::IndexMap::from([(
                    "T".to_string(),
                    baml_type::RuntimeTy::int(),
                )]))
                .build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::String(s)) => assert_eq!(s.as_str(), "7"),
        other => panic!("expected Ok(String(\"7\")) from baml.json.to_string<int>, got {other:?}"),
    }
}
