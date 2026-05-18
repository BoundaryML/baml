//! BEP-034 Phase G: spawn body throws → `await` re-throws.
//!
//! Per BEP-034: "If it failed, `await` re-throws the error." The body's
//! E parameter (`Future<T, E>`) is the union of types it might throw,
//! and the awaiter's catch clause must be able to handle them as
//! ordinary thrown values.
//!
//! State observation after the catch fires (`f.is_error()`, `f.state()`)
//! lives in `future_methods.rs`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn await_uncaught_bubbles_to_host() {
    // No catch — the Io error from the spawn body re-thrown by `await`
    // surfaces as `EngineError::UnhandledThrow` to the host with the
    // original class and field values intact.
    let output = baml_test!(
        r#"
        function boom() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "boom" }
        }
        function main() -> int throws baml.errors.Io {
            let f = spawn { boom() };
            await f
        }
        "#
    );
    let err = output.result.expect_err("expected unhandled Io throw");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("baml.errors.Io"),
        "expected Io class in error, got {msg}",
    );
    assert!(
        msg.contains("boom"),
        "expected original message in error, got {msg}",
    );
}

#[tokio::test]
async fn await_rethrows_caught_by_user_catch() {
    // The re-thrown error from `await` must be catchable via user catch
    // arms. `(await f) catch (e) { … }` parses as expected (parentheses
    // required because `await` is a prefix that would otherwise consume
    // the entire trailing expression including the catch). The engine's
    // fire-and-forget propagation has a carve-out so that an explicit
    // await of an errored future doesn't pre-empt the catch.
    let output = baml_test!(
        r#"
        function boom() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "x" }
        }
        function main() -> int {
            let f = spawn { boom() };
            (await f) catch (e) { baml.errors.Io => 99 }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}
