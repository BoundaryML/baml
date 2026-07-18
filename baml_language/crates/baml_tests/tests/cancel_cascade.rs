//! BEP-034: cancellation race tests.
//!
//! Note: fire-and-forget error propagation surfaces as a host-level
//! `UnhandledThrow` that bypasses all user `catch` expressions in BAML
//! (even wildcard `_ => ...`). The test's assertion `output.result.is_err()`
//! is a host-level observation with no BAML-side equivalent.

use baml_tests::baml_test;

/// BEP-034: "If a fire-and-forget task throws an unhandled error, the
/// error propagates to the parent task at its next `await` point."
///
/// `bad` is spawned with no handle binding; its body throws Io. We then
/// `await waiter` — which is what triggers the propagation. The await
/// must surface the Io error from `bad` instead of returning waiter's
/// successful value.
#[tokio::test]
async fn fire_and_forget_error_surfaces_at_next_await() {
    let output = baml_test!(
        r#"
        function boom() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "boom" }
        }
        function main() -> int {
            let _ = spawn { boom() };
            // Yield so the fire-and-forget thread runs and pushes its
            // error into our pending_child_errors queue before our next
            // await checkpoint observes it.
            await spawn { 99 }
        }
        "#
    );
    // The await on the trivial spawn must surface boom()'s Io error.
    assert!(
        output.result.is_err(),
        "expected Io error propagation, got {:?}",
        output.result,
    );
}
