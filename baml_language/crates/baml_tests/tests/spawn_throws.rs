//! BEP-034 Phase G: spawn body throws → `await` re-throws.
//!
//! Per BEP-034: "If it failed, `await` re-throws the error." The body's
//! E parameter (`Future<T, E>`) is the union of types it might throw,
//! and the awaiter's catch clause must be able to handle them as
//! ordinary thrown values.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn await_uncaught_io_error_bubbles_to_host() {
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
async fn await_uncaught_user_error_bubbles_to_host() {
    // User-defined error class round-trips through the future.
    let output = baml_test!(
        r#"
        class MyErr { code int  why string }
        function fail() -> int throws MyErr {
            throw MyErr { code: 7, why: "nope" }
        }
        function main() -> int throws MyErr {
            await spawn { fail() }
        }
        "#
    );
    let err = output.result.expect_err("expected unhandled MyErr throw");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("MyErr") && msg.contains("nope"),
        "expected MyErr with field values, got {msg}",
    );
}

#[tokio::test]
async fn await_rethrows_caught_by_user_catch() {
    // The re-thrown error from `await` must be catchable via user
    // catch arms. `(await f) catch (e) { … }` parses as expected
    // (parentheses required because `await` is a prefix that would
    // otherwise consume the entire trailing expression including the
    // catch). The engine's fire-and-forget propagation has a
    // carve-out so that an explicit await of an errored future doesn't
    // pre-empt the catch.
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

#[tokio::test]
async fn await_rethrows_user_class_caught_by_user_catch() {
    // User-defined error type also flows through to the catch.
    let output = baml_test!(
        r#"
        class MyErr { code int }
        function fail() -> int throws MyErr {
            throw MyErr { code: 7 }
        }
        function main() -> int {
            let f = spawn { fail() };
            (await f) catch (e) { MyErr => 99 }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn errored_future_is_error_returns_true() {
    // After the awaiter swallowed the error via catch, the heap
    // Future is in Error state and `is_error()` returns true.
    let output = baml_test!(
        r#"
        function fail() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "x" }
        }
        function main() -> bool {
            let f = spawn { fail() };
            let _ = (await f) catch (e) { baml.errors.Io => 0 };
            f.is_error()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn errored_future_state_returns_error_variant() {
    let output = baml_test!(
        r#"
        function fail() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "x" }
        }
        function main() -> baml.future.FutureState {
            let f = spawn { fail() };
            let _ = (await f) catch (e) { baml.errors.Io => 0 };
            f.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Error");
        }
        other => panic!("expected Variant Error, got {other:?}"),
    }
}

#[tokio::test]
async fn direct_catch_handles_thrown_error() {
    // Sanity check: direct-call catch (no await) works as expected.
    // Pairs with the `KNOWN ISSUE` doc above to bound the catch+await
    // bug to the await side.
    let output = baml_test!(
        r#"
        function boom() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "x" }
        }
        function main() -> int {
            boom() catch (e) { baml.errors.Io => -1 }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}
