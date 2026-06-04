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
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
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
            let b = mk(5);
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
