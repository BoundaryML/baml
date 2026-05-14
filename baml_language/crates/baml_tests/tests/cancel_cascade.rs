//! BEP-034: cancellation race tests.
//!
//! Audit-driven additions covering paths the earlier `spawn_basic` /
//! `future_methods` tests didn't reach: a thread whose own cancel token
//! fires while it is suspended in `await` on a future that's NOT a
//! descendant must short-circuit instead of hanging on the SetOnce.

use std::time::{Duration, Instant};

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// `waiter` awaits `slow` (a 60s sibling, not a child of waiter).
/// Cancelling `waiter` fires waiter's own cancel token; without the
/// await-race fix, waiter's `await slow` would block on the SetOnce
/// until `slow` settles 60s later.
#[tokio::test]
async fn cancel_unblocks_await_on_non_descendant() {
    let started = Instant::now();
    let output = baml_test!(
        r#"
        function main() -> int {
            let slow = spawn { baml.sys.sleep(60000); 42 };
            let waiter = spawn { await slow };
            let _ = waiter.cancel();
            // waiter's BexThread is parked in the engine's await on
            // `slow`. cancel() fires waiter's cancel token; the engine
            // race must observe it and settle waiter immediately.
            await waiter
        }
        "#
    );
    let elapsed = started.elapsed();

    // `await waiter` throws Cancelled because waiter is in Cancelled
    // state. The unhandled throw bubbles to the host as an EngineError.
    assert!(
        output.result.is_err(),
        "expected unhandled Cancelled throw, got {:?}",
        output.result,
    );
    // Without the fix, waiter would hang ~60s waiting for slow to
    // settle. Generous bound for CI jitter; the fix should land in tens
    // of ms.
    assert!(
        elapsed < Duration::from_secs(5),
        "cancel-on-await took {}ms (expected near-instant; await-race fix missing?)",
        elapsed.as_millis(),
    );
}

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

/// BEP-034 Phase G `cancel_from_handle.rs` from the original plan:
/// `let f = spawn { sleep(1s); 42 }; f.cancel(); await f catch
/// (Cancelled) { 0 }` returns 0. Confirms the full round-trip from
/// user-side cancel → await throws Cancelled → user catches.
#[tokio::test]
async fn cancel_from_handle_then_await_catches_cancelled() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let f = spawn { baml.sys.sleep(60000); 42 };
            let _ = f.cancel();
            (await f) catch (e) { baml.panics.Cancelled => 0 }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

/// State observation companion to the test above. After `waiter` is
/// cancelled mid-`await`, its heap Future state must reflect Cancelled
/// (not stay Pending), so any subsequent state inspection or re-await
/// behaves correctly.
#[tokio::test]
async fn cancelled_child_future_state_is_cancelled() {
    let output = baml_test!(
        r#"
        function main() -> baml.future.FutureState {
            let slow = spawn { baml.sys.sleep(60000); 42 };
            let waiter = spawn { await slow };
            let _ = waiter.cancel();
            waiter.state()
        }
        "#
    );
    match output.result {
        Ok(BexExternalValue::Variant { variant_name, .. }) => {
            assert_eq!(variant_name, "Cancelled");
        }
        other => panic!("expected Variant Cancelled, got {other:?}"),
    }
}
