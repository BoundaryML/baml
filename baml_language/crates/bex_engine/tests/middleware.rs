//! BEP-034 `with` middleware: `SpawnParams` transformer pipelines.

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

/// A TYPE-CHANGING transformer (the BEP's `withFallback`): wraps the body
/// with a catch-all, so the error type is erased — the spawn types
/// `Future<int, never>` and the failing body resolves to the default.
#[tokio::test]
async fn type_changing_fallback_transformer() {
    let source = r#"
        function withFallback<T, E>(default_value: T) -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, never> throws never {
            (params) -> {
                let original = params.body;
                baml.spawn.SpawnParams {
                    body: () -> {
                        (original()) catch (e) {
                            let e => default_value
                        }
                    },
                    name: params.name,
                    group: params.group,
                    cancel: params.cancel,
                    detach: params.detach,
                }
            }
        }
        function flaky(n: int) -> int throws string {
            if (n > 0) {
                throw "boom"
            }
            n
        }
        function main() -> int {
            let f = spawn with withFallback(99) { flaky(1) };
            // `f: Future<int, never>` — awaiting needs no catch.
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// The pipeline runs transformers EAGERLY in the spawner, so a transformer
/// that throws does so at the spawn site — catchable there, and part of the
/// CALLER's throws surface (an explicit `throws never` caller is rejected;
/// see the TIR throws contribution).
#[tokio::test]
async fn throwing_transformer_throws_at_spawn_site() {
    let source = r#"
        function withBomb<T, E>() -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws string throws never {
            (params) -> { throw "cfg-exploded" }
        }
        function main() -> string {
            let r = (spawn with withBomb() { 1 }) catch (e) {
                let s: string => s
            };
            // The body never ran; the transformer's throw surfaced at spawn.
            match (r) {
                let s: string => s,
                _ => "wrong"
            }
        }
    "#;
    assert_eq!(
        run_main(source).await.unwrap(),
        BexExternalValue::String("cfg-exploded".into())
    );
}

/// A transformer bound to a VARIABLE first (not called inline). The `let`
/// binding gives generic inference no context to solve `E`, so it is supplied
/// explicitly (`withDouble<never>()` — the spawn body throws nothing, so its
/// `SpawnParams` error type is `never`); the pipeline still runs from the
/// variable.
#[tokio::test]
async fn variable_bound_transformer_still_applies() {
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
            let t = withDouble<never>();
            let f = spawn with t { 21 };
            await f
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}
