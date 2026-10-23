//! A generic function value whose type arguments are inferred from the
//! expected function type (`function get() -> (int) -> int { identity }`) is
//! the same specialized value the written `identity<int>` is. These tests pin
//! the host boundary, which BAML source cannot observe: the value crosses out
//! as a handle stating its specialized type, and the host can call it or pass
//! it back in. In-language behavior (calling the value, `is`, equality) is
//! covered by the corpus in
//! `baml_tests/baml_src/ns_instantiation_expr/inferred_instantiation.baml`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, TaggedHeapHandleKind};
use common::compile_for_engine;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    function identity<T>(x: T) -> T { x }
    function empty_list<T>() -> T[] { [] }

    class Holder {
        function pick<U>(self, u: U) -> U { u }
        function make<U>(u: U) -> U { u }
    }

    function get_identity() -> (int) -> int throws never { identity }
    function get_empty_list() -> () -> string[] throws never { empty_list }
    function get_bound_pick() -> (int) -> int throws never {
        let h = Holder {};
        h.pick
    }
    function get_static_make() -> (int) -> int throws never { Holder.make }
    function get_list_at() -> (index: int) -> int? throws never {
        let xs = [10, 20, 30];
        xs.at
    }
    function get_map_length() -> () -> int throws never {
        let m = { "a": 1 };
        m.length
    }
    function get_future_is_settled() -> () -> bool throws never {
        let f = spawn { 1 };
        let _ = await f;
        f.is_settled
    }

    function apply_one(f: (int) -> int throws never) -> int { f(1) }
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("engine construction"),
    )
}

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

/// Call the zero-argument `getter` and return the function value it builds,
/// converted the way an SDK receives it: a callable handle stating its type.
/// Converting it is a crossing the engine checks (in debug builds) for a
/// complete generic frame.
async fn get_function_value(engine: &Arc<BexEngine>, getter: &str) -> BexExternalValue {
    let value = engine
        .call_function(getter, Vec::new(), context(), true)
        .await
        .unwrap_or_else(|err| panic!("{getter} should return its function value: {err:?}"));
    assert!(
        matches!(
            value,
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                kind: TaggedHeapHandleKind::Callable,
                ..
            })
        ),
        "{getter}'s function value should cross as a callable handle, got {value:?}"
    );
    value
}

/// The heap handle a callable value crossed as.
fn callable_handle(value: BexExternalValue) -> bex_external_types::Handle {
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) = value else {
        unreachable!("checked by get_function_value");
    };
    heap_handle
}

/// A host hands the value back to BAML where `(int) -> int` is declared: the
/// value states that type, so it is admitted and runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_inferred_instantiation_round_trips_through_the_host() {
    let engine = engine();
    for getter in ["get_identity", "get_bound_pick", "get_static_make"] {
        let value = get_function_value(&engine, getter).await;
        let result = engine
            .call_function("apply_one", vec![value], context(), true)
            .await
            .unwrap_or_else(|err| panic!("{getter}'s value inhabits `(int) -> int`: {err:?}"));
        assert_eq!(result, BexExternalValue::Int(1), "{getter}");
    }
}

/// A host calls the value through its handle: an `int` runs, and a `string`
/// is refused at the boundary because the value's parameter is `int`, not
/// the generic's open `T`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_call_checks_arguments_against_the_inferred_instantiation() {
    let engine = engine();
    for getter in ["get_identity", "get_bound_pick", "get_static_make"] {
        let handle = callable_handle(get_function_value(&engine, getter).await);
        let result = engine
            .call_callable(
                handle.clone(),
                vec![BexExternalValue::Int(3)],
                context(),
                true,
            )
            .await
            .unwrap_or_else(|err| panic!("{getter}'s value takes an int: {err:?}"));
        assert_eq!(result, BexExternalValue::Int(3), "{getter}");

        let err = engine
            .call_callable(
                handle,
                vec![BexExternalValue::String("x".into())],
                context(),
                true,
            )
            .await
            .err()
            .unwrap_or_else(|| {
                panic!("{getter}: a string is not an argument of an `(int) -> int` value")
            });
        assert!(
            matches!(err, EngineError::TypeMismatch { .. }),
            "{getter}: expected a type mismatch, got {err:?}"
        );
    }
}

/// A body that reifies its type parameter (`[]` at `T[]`) runs when the host
/// calls an inferred instantiation: the value seeds the callee's frame.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_call_seeds_the_frame_of_an_inferred_instantiation() {
    let engine = engine();
    let handle = callable_handle(get_function_value(&engine, "get_empty_list").await);
    let result = engine
        .call_callable(handle, Vec::new(), context(), true)
        .await
        .expect("the value carries `T = string`");
    let BexExternalValue::Array { items, .. } = result else {
        panic!("expected a list, got {result:?}");
    };
    assert!(items.is_empty());
}

/// A builtin's method value is realized from its receiver: a list's element
/// type, a map's key and value types and a future's value and error types
/// fill its carrier class's frame, so the value crosses to the host complete
/// and runs when the host calls it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn builtin_method_values_carry_the_receivers_type_arguments() {
    let engine = engine();
    for getter in ["get_list_at", "get_map_length", "get_future_is_settled"] {
        let value = get_function_value(&engine, getter).await;
        assert!(
            matches!(
                value,
                BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                    kind: TaggedHeapHandleKind::Callable,
                    ..
                })
            ),
            "{getter}: {value:?}"
        );
    }
    let handle = callable_handle(get_function_value(&engine, "get_future_is_settled").await);
    let result = engine
        .call_callable(handle, Vec::new(), context(), true)
        .await
        .expect("a settled future's `is_settled` runs from the host");
    assert_eq!(result, BexExternalValue::Bool(true));
}
