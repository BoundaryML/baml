//! BEP-034 Phase G: spawn body throws → `await` re-throws.
//!
//! Per BEP-034: "If it failed, `await` re-throws the error." The body's
//! E parameter (`Future<T, E>`) is the union of types it might throw,
//! and the awaiter's catch clause must be able to handle them as
//! ordinary thrown values.
//!
//! KNOWN ISSUE — catch + await: `(await f) catch (e) { … }` does not
//! invoke the catch handler today. The bytecode emitter generates the
//! catch arms after the `await` instruction, but the exception table
//! entry produced by `build_exception_table` does not cover the
//! await's PC, so a re-thrown value from a settled-Error future
//! escapes past the catch and bubbles to the host. Direct-call catch
//! (`boom() catch (e) { … }`) works fine, so the bug is specific to
//! the await catch-region wiring. Tracked separately; tests below
//! cover the throw-round-trip without relying on user-side catch.

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
