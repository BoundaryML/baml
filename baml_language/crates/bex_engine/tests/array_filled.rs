//! Runtime coverage for the `Array.filled(n, value)` stdlib constructor
//! (`baml/containers.baml`). Confirms it allocates a runtime-sized array
//! pre-initialized to `value`, widens the element type from `value`, and
//! clamps non-positive `n` to an empty array.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Compile `source`, run its `main` entry point, and return the result.
///
/// # Parameters
/// - `source`: BAML source that must define a zero-argument `main`.
///
/// # Returns
/// The value produced by `main`, or the [`EngineError`] raised while
/// compiling or executing it.
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

/// `baml.Array.filled(n, value)` returns an array of length `n` with every
/// slot set to `value` (the canonical zero-initialized buffer use case).
#[tokio::test]
async fn array_filled_length_and_value() {
    let source = r#"
        function main() -> int {
            let xs = baml.Array.filled(5, 0);
            let total = xs.length();
            for (let x in xs) {
                if (x != 0) { total += 100; }
            }
            total
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(5));
}

/// A non-positive `n` clamps to an empty array rather than panicking.
#[tokio::test]
async fn array_filled_non_positive_is_empty() {
    let source = r#"
        function main() -> int {
            baml.Array.filled(0, 7).length() + baml.Array.filled(-3, 7).length()
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(0));
}

/// The element type is inferred from `value`, so a string fill yields a
/// `string[]` whose entries are usable as strings.
#[tokio::test]
async fn array_filled_infers_element_type() {
    let source = r#"
        function main() -> string {
            let xs = baml.Array.filled(3, "ab");
            xs.join("-")
        }
    "#;
    assert_eq!(
        run_main(source).await.unwrap(),
        BexExternalValue::String("ab-ab-ab".into())
    );
}

/// Filling with a mutable reference type aliases that same object in each slot:
/// mutating one inner array is visible through all of them.
#[tokio::test]
async fn array_filled_mutable_value_aliases_all_slots() {
    let source = r#"
        function main() -> int {
            let rows = baml.Array.filled(3, [0]);
            match (rows.at(0)) {
                null => 0,
                let first: int[] => {
                    first.push(1);
                    let total = 0;
                    for (let row in rows) {
                        total += row.length();
                    }
                    total
                }
            }
        }
    "#;
    // If each slot had an independent copy we'd get 4 (2 + 1 + 1).
    // The current `Array.filled` contract aliases the same inner array, so
    // all rows have length 2 after one push: 2 + 2 + 2 = 6.
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(6));
}
