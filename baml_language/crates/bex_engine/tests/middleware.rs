//! BEP-034 `with` middleware: `SpawnParams` transformer pipelines.

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

/// The simplest transformer: identity. Exercises the chain plumbing
/// (`SpawnParams` construction, transformer application, engine dispatch
/// from the final params).
#[tokio::test]
async fn identity_transformer() {
    let source = r#"
        function ident<T, E>() -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws never {
            (params) -> { params }
        }
        function main() -> int {
            let f = spawn with ident() { 41 + 1 };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}

/// `baml.spawn.options(...)` is now an ordinary transformer.
#[tokio::test]
async fn options_as_transformer() {
    let source = r#"
        function main() -> int {
            let f = spawn "opted" with baml.spawn.options(detach = false) { 7 };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// A transformer that WRAPS the body — the runtime must run the wrapped
/// body from the final params, not the original closure operand.
#[tokio::test]
async fn body_wrapping_transformer() {
    let source = r#"
        function withDouble<E>() -> (baml.spawn.SpawnParams<int, E>) -> baml.spawn.SpawnParams<int, E> throws never {
            (params) -> {
                let original = params.body;
                baml.spawn.SpawnParams {
                    body: () -> { original() * 2 },
                    name: params.name,
                    group: params.group,
                    cancel: params.cancel,
                    detach: params.detach,
                }
            }
        }
        function main() -> int {
            let f = spawn with withDouble() { 10 };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(20));
}

/// Pipeline order is left-to-right: each transformer wraps the body the
/// previous one produced, so the LAST one's wrap runs outermost.
/// `withDouble, withAddOne` → addOne(double(10)) = 21 (not 22).
#[tokio::test]
async fn chained_transformers_apply_left_to_right() {
    let source = r#"
        function withDouble<E>() -> (baml.spawn.SpawnParams<int, E>) -> baml.spawn.SpawnParams<int, E> throws never {
            (params) -> {
                let original = params.body;
                baml.spawn.SpawnParams {
                    body: () -> { original() * 2 },
                    name: params.name,
                    group: params.group,
                    cancel: params.cancel,
                    detach: params.detach,
                }
            }
        }
        function withAddOne<E>() -> (baml.spawn.SpawnParams<int, E>) -> baml.spawn.SpawnParams<int, E> throws never {
            (params) -> {
                let original = params.body;
                baml.spawn.SpawnParams {
                    body: () -> { original() + 1 },
                    name: params.name,
                    group: params.group,
                    cancel: params.cancel,
                    detach: params.detach,
                }
            }
        }
        function main() -> int {
            let f = spawn with withDouble(), withAddOne() { 10 };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(21));
}

/// The BEP's marquee example: user-written retry middleware. The body
/// fails twice then succeeds; `withRetry(5)` re-runs it until it does.
#[tokio::test]
async fn custom_with_retry() {
    let source = r#"
        function flaky_then_42(calls: int[]) -> int throws string {
            calls.push(1);
            if (calls.length() < 3) {
                throw "flaky"
            }
            42
        }

        function withRetry<T, E>(n: int) -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws never {
            (params) -> {
                let original = params.body;
                baml.spawn.SpawnParams {
                    body: () -> {
                        let winners: T[] = [];
                        let attempts = 0;
                        while (winners.length() == 0 && attempts < n - 1) {
                            attempts = attempts + 1;
                            (winners.push(original())) catch (e) {
                                let e => {}
                            };
                        }
                        if (winners.length() > 0) {
                            winners[0]
                        } else {
                            original()
                        }
                    },
                    name: params.name,
                    group: params.group,
                    cancel: params.cancel,
                    detach: params.detach,
                }
            }
        }

        function main() -> int {
            let calls: int[] = [];
            let f = spawn "retry-test" with withRetry(5) { flaky_then_42(calls) };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}
