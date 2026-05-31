//! BEP-034: `await` distributes over unions of futures.
//!
//! `Future` is invariant, so combining differently-typed futures (if/else,
//! array elements) yields `Future<A, E1> | Future<B, E2>`; awaiting it gives
//! value `A | B` and error `E1 | E2`.

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

/// The BEP's motivating shape: an if/else picking between differently-typed
/// futures. The await's value type is the union of the members' value types.
#[tokio::test]
async fn await_union_of_futures_value_side() {
    let source = r#"
        function as_int() -> int { 41 }
        function as_string() -> string { "x" }
        function pick(use_int: bool) -> int | string {
            let f = if (use_int) { spawn { as_int() } } else { spawn { as_string() } };
            await f
        }
        function main() -> int {
            let v = pick(true);
            match (v) {
                let n: int => n + 1,
                _ => -1
            }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}

/// Error side: each member's `E` is part of the await's escaping throws, so a
/// `throws never` contract on the awaiting fn is violated by EITHER error
/// type — and an undeclared fn forwards the union.
#[tokio::test]
async fn await_union_of_futures_error_side() {
    let source = r#"
        function bad_a() -> int throws string { throw "a" }
        function bad_b() -> int throws int { throw 7 }
        function pick(use_a: bool) -> int {
            let f = if (use_a) { spawn { bad_a() } } else { spawn { bad_b() } };
            (await f) catch (e) {
                let s: string => 1,
                let n: int => n
            }
        }
        function main() -> int {
            pick(false)
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}
