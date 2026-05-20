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

use baml_project::testing::compile_multi_file;
use bex_engine::{BexEngine, CallId, EngineError, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

fn engine(files: &[(&str, &str)]) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_multi_file(files),
            Arc::new(sys_native::SysOps::native()),
            None,
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

/// `BexEngine::call_function*` requires a [`FunctionKind::Bytecode`]
/// callee — `$rust_function` natives, sysops, and unresolved natives
/// have no enclosing bytecode frame to `YieldToCall` back into, so
/// dispatch can't honor a yield. Without the gate, a future host
/// calling e.g. `baml.json.to_string<T>` directly would crash deep in
/// the VM with a misleading internal error. Catching it at the engine
/// boundary surfaces a clean, documented `NotInvokableAsEntry`.
#[tokio::test]
async fn call_function_rejects_non_bytecode_entry() {
    let eng = engine(&[("main.baml", "function main() -> int { 1 }")]);

    // `baml.json.to_string` is a `$rust_function` native (see
    // baml_builtins2/baml_std/baml/ns_json/json.baml). Invoking it as
    // an entry must be rejected up-front.
    let result = eng
        .call_function(
            "baml.json.to_string",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(EngineError::NotInvokableAsEntry { name, .. }) => {
            assert_eq!(name, "baml.json.to_string");
        }
        other => panic!("expected NotInvokableAsEntry, got {other:?}"),
    }

    // Sanity: bytecode entries (including LLM-typed bytecode, which is
    // `FunctionKind::Bytecode + FunctionMeta::Llm`) still work — this
    // gate only blocks `Native`/`SysOp`/`NativeUnresolved`.
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
