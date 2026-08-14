//! Runtime reified-generics threading: inferred call-site type args populate
//! `frame.type_args`, instances constructed in generic frames carry resolved
//! `class_type_args`, and typed patterns naming the enclosing function's
//! `TypeVar`s compile to `TypeArgRef` tests (not constant-false `Void` tests).

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// Call a named function with explicit *named* `TypeVar` bindings (the host SDK
/// `_types=` channel) and the given argument values. The inbound surface that
/// 01pt3 makes generics-aware.
async fn call_named(
    source: &str,
    function: &str,
    args: Vec<BexExternalValue>,
    type_args: Vec<(String, baml_type::RuntimeTy)>,
) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_type_args(type_args.into_iter().collect())
                .build(),
            true,
        )
        .await
}

/// An instance constructed inside a generic frame (`Box<T>` in `mk<T>`)
/// carries the caller-resolved type args, observable via `is` after the
/// value escapes to a non-generic frame. The `mk("hi")` call writes no
/// explicit type args — `T = string` is threaded from TIR's inferred
/// instantiation.
#[tokio::test]
async fn escaped_instance_carries_inferred_type_args() {
    let source = r#"
        class Box<T> { v T }
        function mk<T>(x: T) -> Box<T> {
            let b: Box<T> = Box { v: x };
            b
        }
        function main() -> int {
            let b = mk("hi");
            if (b is Box<string>) { 1 } else { 0 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(1));
}

/// Negative control for the test above: the same escaped instance does NOT
/// match a differently-instantiated type.
#[tokio::test]
async fn escaped_instance_rejects_wrong_type_args() {
    let source = r#"
        class Box<T> { v T }
        function mk<T>(x: T) -> Box<T> {
            let b: Box<T> = Box { v: x };
            b
        }
        function main() -> int {
            let b: unknown = mk(5);
            if (b is Box<string>) { 1 } else { 0 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(0));
}

/// A typed catch arm naming the ENCLOSING function's `TypeVar`
/// (`AllFailed<E>` inside `helper<E>`) matches at runtime: the pattern
/// lowers to a `TypeArgRef` template resolved against helper's frame
/// (`E = string`, inferred at main's call site), and the instance thrown by
/// generic `any` carries `class_type_args = [string]` through the
/// main → helper → any inferred generic chain.
#[tokio::test]
async fn typed_arm_on_enclosing_typevar_matches() {
    let source = r#"
        function bad() -> int throws string { throw "x" }
        function helper<E>(fs: baml.future.Future<int, E>[]) -> int {
            (await baml.future.any(fs)) catch (e) {
                let e: baml.future.AllFailed<E> => e.errors.length()
            }
        }
        function main() -> int {
            helper([spawn { bad() }, spawn { bad() }])
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(2));
}

// ===========================================================================
// 01pt3: inbound (host-call) generics — named `_types=` channel
// ===========================================================================

/// A body-only `TypeVar` (`T` appears only via `reflect.type_of<T>()`, never in
/// the signature) is bound through the named channel and threads into the
/// frame. Proves the path doesn't rely on argument inference.
#[tokio::test]
async fn inbound_named_binding_threads_to_reflect() {
    let source = r#"
        function one_type_arg<T>() -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let out = call_named(
        source,
        "one_type_arg",
        vec![],
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

/// A generic function called without binding all its type params is a hard
/// caller error at the inbound boundary (the wire must be fully bound).
#[tokio::test]
async fn inbound_missing_binding_rejected() {
    let source = r#"
        function one_type_arg<T>() -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let err = call_named(source, "one_type_arg", vec![], vec![])
        .await
        .expect_err("missing _types binding must be rejected");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

/// A generic instance argument's wire-supplied class type args are landed into
/// the VM `Object::Instance::class_type_args` (the `alloc_instance` fix). The
/// callee tests `b is Box<T>`: this matches only if the instance's resolved
/// class type args (`[int]`, from the wire) equal the frame's `T` binding
/// (`int`, from the named channel) — i.e. the wire args flowed into the VM
/// instance. (Were the args dropped, `class_type_args` would be `[]` and the
/// length-mismatch would make the test false.)
#[tokio::test]
async fn inbound_instance_arg_lands_class_type_args() {
    let source = r#"
        class Box<T> { v T }
        function takes_box<T>(b: Box<T>) -> int {
            if (b is Box<T>) { 1 } else { 0 }
        }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "Box",
        vec![baml_type::RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("v", BexExternalValue::Int(5))]),
    );
    let out = call_named(
        source,
        "takes_box",
        vec![arg],
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(1));
}

/// A generic instance whose wire class args disagree with the (substituted)
/// declared param type is a hard type error (`Box<string>` vs `Box<int>`).
#[tokio::test]
async fn inbound_instance_arg_wrong_type_args_rejected() {
    let source = r#"
        class Box<T> { v T }
        function takes_box<T>(b: Box<T>) -> int { 1 }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "Box",
        vec![baml_type::RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("v", BexExternalValue::String("hi".into()))]),
    );
    let err = call_named(
        source,
        "takes_box",
        vec![arg],
        // bind T = int, but the instance carries Box<string>
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .expect_err("Box<string> arg against Box<int> param must be rejected");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

/// A shape-only generic instance inherits its concrete arguments from the
/// contextual class slot. Sparse inbound annotations are only required when
/// that context and the payload do not determine the arguments.
#[tokio::test]
async fn inbound_instance_arg_missing_type_args_uses_context() {
    let source = r#"
        class Box<T> { v T }
        function takes_box<T>(b: Box<T>) -> int { 1 }
        function main() -> int { 0 }
    "#;
    // `instance` == `instance_generic` with empty type_args.
    let arg = BexExternalValue::instance(
        "Box",
        indexmap::IndexMap::from_iter([("v", BexExternalValue::Int(5))]),
    );
    let out = call_named(
        source,
        "takes_box",
        vec![arg],
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .expect("the concrete Box<int> context supplies the omitted type arg");
    assert_eq!(out, BexExternalValue::Int(1));
}

/// A generic instance whose wire arity disagrees with the declared class is
/// rejected (the tightened `class_type_args_compatible` arity check). A
/// two-arg `Box` value can't inhabit the single-param `Box<int>` slot.
#[tokio::test]
async fn inbound_instance_arg_wrong_arity_rejected() {
    let source = r#"
        class Box<T> { v T }
        function takes_box<T>(b: Box<T>) -> int { 1 }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "Box",
        vec![baml_type::RuntimeTy::int(), baml_type::RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("v", BexExternalValue::Int(5))]),
    );
    let err = call_named(
        source,
        "takes_box",
        vec![arg],
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .expect_err("a two-arg Box against Box<int> must be rejected");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

/// A bare host map can be promoted into a generic class when the contextual
/// class slot already supplies the concrete arguments.
#[tokio::test]
async fn inbound_bare_map_against_generic_class_uses_context() {
    let source = r#"
        class Box<T> { v T }
        function takes_box<T>(b: Box<T>) -> int { 1 }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::int(),
        entries: indexmap::IndexMap::from_iter([("v".to_string(), BexExternalValue::Int(5))]),
    };
    let out = call_named(
        source,
        "takes_box",
        vec![arg],
        vec![("T".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .expect("the contextual Box<int> type supplies the map's nominal identity");
    assert_eq!(out, BexExternalValue::Int(1));
}

/// Explicit call-site type args still win over inference (regression guard
/// for the explicit path the inferred fallback was added next to).
#[tokio::test]
async fn explicit_type_args_still_thread() {
    let source = r#"
        class Box<T> { v T }
        function mk<T>(x: T) -> Box<T> {
            let b: Box<T> = Box { v: x };
            b
        }
        function main() -> int {
            let b = mk<string>("hi");
            if (b is Box<string>) { 1 } else { 0 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(1));
}
