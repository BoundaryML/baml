//! Cancellation coverage for `throws_test.SleepMs`.
//!
//! PROVISIONAL API: the Rust bridge has not pinned its explicit-cancellation
//! surface yet. This port assumes a `baml_bridge::runtime::BamlCallContext`
//! handle with `abort()`, threaded into a call through a `*_with_ctx`
//! sibling — the analogue of python's `_ctx=` keyword argument. (A
//! `baml_bridge::runtime::cancel_function_call(call_id)`-style free function is
//! the other candidate shape.) Expect fixups here when the surface lands.

use std::time::{Duration, Instant};

use baml_bridge::runtime::BamlCallContext;
use baml_sdk::throws_test;

// The cancelled calls below sleep 60s: the operation must dwarf this bound, or a
// regression that ignored cancellation would still finish inside it and pass.
const _MAX_CANCELLATION_SECONDS: f64 = 5.0;

/// python asserts `isinstance(exc.value, Cancelled)`; `baml_bridge::Error::Panic`
/// carries only the rendered message + trace, so the class check adapts to
/// the message naming the `Cancelled` panic class (provisional until
/// structured panic payloads are pinned).
fn _assert_cancelled_panic<E: std::fmt::Debug>(exc: baml_bridge::Error<E>) {
    match exc {
        baml_bridge::Error::Panic { message, .. } => {
            assert!(message.contains("Cancelled"), "{message}");
        }
        other => panic!("expected Error::Panic, got {other:?}"),
    }
}

/// DIVERGENCE(rust): asyncio delivers a context abort as a `CancelledError`
/// whose `reason` wraps the `Cancelled` panic; tokio has no cross-task
/// exception injection, so the aborted call itself returns the cancellation
/// panic and the reason check collapses onto [`_assert_cancelled_panic`].
fn _assert_cancelled_reason<E: std::fmt::Debug>(exc: baml_bridge::Error<E>) {
    _assert_cancelled_panic(exc);
}

fn _assert_fast_cancellation(start: Instant) {
    let elapsed = start.elapsed().as_secs_f64();
    assert!(
        elapsed < _MAX_CANCELLATION_SECONDS,
        "cancellation took {elapsed:.3}s"
    );
}

#[test]
fn test_cancellation_sync_call_returns_none() {
    // `SleepMs` returns `null`: the `Result<(), _>` unwrap is the `is None`.
    throws_test::SleepMs(1).unwrap();
}

#[tokio::test]
async fn test_cancellation_async_call_returns_none() {
    throws_test::SleepMs_async(1).await.unwrap();
}

#[test]
fn test_cancellation_sync_cancel_via_call_context() {
    let start = Instant::now();
    let ctx = BamlCallContext::new();

    // python arms a `threading.Timer` and cancels it in `finally`; a scoped
    // thread is joined instead (aborting a context whose call has already
    // finished is a no-op).
    std::thread::scope(|scope| {
        let timer = scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            ctx.abort();
        });

        // PROVISIONAL: `_ctx=ctx` → the `_with_ctx` sibling.
        let result = throws_test::SleepMs_with_ctx(60000, &ctx);
        _assert_cancelled_panic(result.unwrap_err());
        timer.join().unwrap();
    });

    _assert_fast_cancellation(start);
}

#[tokio::test]
async fn test_cancellation_async_cancel_via_call_context() {
    let start = Instant::now();
    let ctx = BamlCallContext::new();

    // python cancels a spawned task and catches `asyncio.CancelledError`;
    // here the call and the aborter run under `join!` and the aborted call
    // itself resolves to the cancellation error.
    // PROVISIONAL: `_ctx=ctx` → the `_with_ctx` sibling.
    let (result, ()) = tokio::join!(throws_test::SleepMs_async_with_ctx(60000, &ctx), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        ctx.abort();
    });

    _assert_cancelled_reason(result.unwrap_err());
    _assert_fast_cancellation(start);
}

#[tokio::test]
async fn test_cancellation_async_cancel_via_task_cancel() {
    let start = Instant::now();
    let task = tokio::spawn(throws_test::SleepMs_async(60000));

    tokio::time::sleep(Duration::from_millis(50)).await;
    task.abort();

    // python awaits the cancelled task and catches `asyncio.CancelledError`;
    // tokio surfaces the abort as a cancelled `JoinError`.
    let join_err = task.await.unwrap_err();
    assert!(join_err.is_cancelled());

    _assert_fast_cancellation(start);
}

#[tokio::test]
async fn test_cancellation_async_cancel_via_task_group_sibling() {
    let start = Instant::now();

    async fn _fail_soon() -> Result<(), &'static str> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Err("cancel siblings")
    }

    // DIVERGENCE(rust): tokio has no `TaskGroup`/`ExceptionGroup`;
    // `try_join!` carries the same sibling semantics — the first failure
    // resolves the join (its error is the ExceptionGroup's `RuntimeError`)
    // and drops the still-running BAML call, which is the analogue of
    // `task.cancelled()`.
    let result = tokio::try_join!(
        async {
            throws_test::SleepMs_async(60000)
                .await
                .map_err(|_| "sleep failed")
        },
        _fail_soon(),
    );

    assert_eq!(result.unwrap_err(), "cancel siblings");
    _assert_fast_cancellation(start);
}

#[tokio::test]
async fn test_cancellation_async_cancel_via_asyncio_timeout() {
    let start = Instant::now();
    // `asyncio.wait_for(..., timeout=0.05)` → `tokio::time::timeout`; the
    // elapsed error is the `TimeoutError`, and the timed-out call future is
    // dropped (cancelled).
    let result =
        tokio::time::timeout(Duration::from_millis(50), throws_test::SleepMs_async(60000)).await;
    assert!(result.is_err());

    _assert_fast_cancellation(start);
}
