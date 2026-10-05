//! A host argument that does not inhabit its declared type is rejected before
//! the function runs. The error names the function and the 1-based position
//! of the argument, wherever in the argument the value of another kind sits.

mod common;

use std::sync::Arc;

use baml_type::RuntimeTy;
use bex_engine::{BexEngine, BexExternalValue as BEV, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    class Reading {
        label: string
        count: int
    }

    function Twice(n: int) -> int {
        n * 2
    }

    function Count(prefix: string, reading: Reading) -> int {
        reading.count
    }

    function MakeAdder(offset: int) -> (value: int) -> int throws never {
        return (x: int) -> int { offset + x }
    }

    function TwiceAsValue() -> (n: int) -> int throws never {
        Twice
    }
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    )
}

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn type_mismatch(error: EngineError) -> String {
    let EngineError::TypeMismatch { message } = error else {
        panic!("expected TypeMismatch, got {error:?}");
    };
    message
}

/// The message of the type mismatch that a call of `function` with `args`
/// ends with.
async fn mismatch_message(function: &str, args: Vec<BEV>) -> String {
    type_mismatch(
        engine()
            .call_function(function, args, context(), true)
            .await
            .expect_err("the argument does not inhabit its declared type"),
    )
}

/// The message of the type mismatch that a call of the callable which
/// `factory(factory_args)` returns ends with, for `args`.
async fn callable_mismatch_message(
    factory: &str,
    factory_args: Vec<BEV>,
    args: Vec<BEV>,
) -> String {
    let engine = engine();
    let callable = engine
        .call_function(factory, factory_args, context(), false)
        .await
        .expect("the factory returns a callable");
    let BEV::Handle(callable) = callable else {
        panic!("expected a handle to a callable, got {callable:?}");
    };
    type_mismatch(
        engine
            .call_callable(callable, args, context(), true)
            .await
            .expect_err("the argument does not inhabit its declared type"),
    )
}

fn string(value: &str) -> BEV {
    BEV::String(value.into())
}

/// A map as a host without a generated class sends it: string keys, and no
/// statement about the type of the values.
fn untyped_map(entries: &[(&str, BEV)]) -> BEV {
    BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::Unknown,
        entries: entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    }
}

#[tokio::test]
async fn scalar_of_another_kind_names_the_function_and_the_argument() {
    assert_eq!(
        mismatch_message("Twice", vec![string("7")]).await,
        "`Twice` was called with a value that doesn't match its type: argument 1: \
         Value of type 'string' does not match the declared type `int`"
    );
}

/// The fields of a class are checked when the map becomes an instance, after
/// the check of the argument as a whole.
#[tokio::test]
async fn class_field_of_another_kind_names_the_function_and_the_argument() {
    let reading = untyped_map(&[("label", string("a")), ("count", string("seven"))]);
    assert_eq!(
        mismatch_message("Count", vec![string("p"), reading]).await,
        "`Count` was called with a value that doesn't match its type: argument 2: \
         Value of type 'string' does not match the declared type `int`"
    );
}

/// A function that the host holds as a value is called through its handle.
/// The mismatch names it as a call by name does.
#[tokio::test]
async fn function_value_names_the_function_and_the_argument() {
    assert_eq!(
        callable_mismatch_message("TwiceAsValue", vec![], vec![string("7")]).await,
        "`Twice` was called with a value that doesn't match its type: argument 1: \
         Value of type 'string' does not match the declared type `int`"
    );
}

/// A closure has no name that a user wrote.
#[tokio::test]
async fn closure_names_the_argument() {
    assert_eq!(
        callable_mismatch_message("MakeAdder", vec![BEV::Int(10)], vec![string("7")]).await,
        "the callable was called with a value that doesn't match its type: argument 1: \
         Value of type 'string' does not match the declared type `int`"
    );
}
