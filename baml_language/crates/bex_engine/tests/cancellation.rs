//! Tests for cancellation support in the BEX engine.
//!
//! Verifies that `CancellationToken` correctly interrupts function execution
//! at various points: immediately, during sleep, during HTTP, and across
//! retry/fallback orchestration strategies.

mod common;

use std::sync::Arc;

use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, EngineError, FunctionCallContextBuilder,
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

// ============================================================================
// 1. Immediate cancellation — token already cancelled before call starts
// ============================================================================

#[tokio::test]
async fn cancel_before_call_returns_cancelled() {
    // call_function checks the token before starting the VM, so even a
    // purely synchronous function returns Cancelled immediately.
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
        .expect("Failed to create engine");

    let cancel = CancellationToken::new();
    cancel.cancel(); // Cancel before the call

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
        )
        .await;

    assert!(
        matches!(result, Err(EngineError::Cancelled)),
        "Expected EngineError::Cancelled, got: {result:?}"
    );
}

// ============================================================================
// 2. Cancellation during sleep — engine should exit promptly
// ============================================================================

#[tokio::test]
async fn cancel_during_sleep_returns_promptly() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(10000);
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
            .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let start = std::time::Instant::now();

    let handle = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel_clone)
                        .build(),
                )
                .await
        }
    });

    // Give the function time to start the sleep, then cancel.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(EngineError::Cancelled)),
        "Expected EngineError::Cancelled, got: {result:?}"
    );
    // Should return well before the 10s sleep completes.
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "Cancel took too long: {elapsed:?} (expected < 2s)"
    );
}

// ============================================================================
// 3. Cancellation during HTTP — engine should exit promptly
// ============================================================================

#[tokio::test]
async fn cancel_during_http_returns_promptly() {
    // Start a mock server that delays 10s before responding.
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/slow"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string("done")
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> string {{
            let response = baml.http.fetch("{}/slow");
            response.text()
        }}
        "#,
        mock_server.uri()
    );

    let snapshot = compile_for_engine(&source);
    let engine = Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
            .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let start = std::time::Instant::now();

    let handle = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel_clone)
                        .build(),
                )
                .await
        }
    });

    // Give the function time to start the HTTP request, then cancel.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(EngineError::Cancelled)),
        "Expected EngineError::Cancelled, got: {result:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "Cancel took too long: {elapsed:?} (expected < 2s)"
    );
}

// ============================================================================
// 4. Selective cancellation — cancel one call, others complete
// ============================================================================

#[tokio::test]
async fn selective_cancellation_only_affects_target() {
    let source = r#"
        function slow() -> int {
            baml.sys.sleep(5000);
            1
        }

        function fast() -> int {
            2
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
            .expect("Failed to create engine"),
    );

    let cancel_slow = CancellationToken::new();
    let cancel_fast = CancellationToken::new();

    let handle_slow = tokio::spawn({
        let engine = Arc::clone(&engine);
        let cancel = cancel_slow.clone();
        async move {
            engine
                .call_function(
                    "slow",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel)
                        .build(),
                )
                .await
        }
    });

    let handle_fast = tokio::spawn({
        let engine = Arc::clone(&engine);
        let cancel = cancel_fast.clone();
        async move {
            engine
                .call_function(
                    "fast",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel)
                        .build(),
                )
                .await
        }
    });

    // Cancel only the slow call.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel_slow.cancel();

    let result_slow = handle_slow.await.expect("task panicked");
    let result_fast = handle_fast.await.expect("task panicked");

    assert!(
        matches!(result_slow, Err(EngineError::Cancelled)),
        "Slow call should be cancelled, got: {result_slow:?}"
    );
    assert_eq!(
        result_fast.expect("fast call failed"),
        BexExternalValue::Int(2),
        "Fast call should complete normally"
    );
}

// ============================================================================
// 5. Cooperative cancellation_requested check — BAML code sees the cancellation
// ============================================================================

#[tokio::test]
async fn cancellation_requested_returns_false_when_not_cancelled() {
    // cancellation_requested() is a sync sys_op (SysOpOutput::ok), so it takes
    // the Ready path: ScheduleFuture → set_future_ready → Await sees Ready →
    // extracts value without entering the biased select.
    let source = r#"
        function main() -> bool {
            baml.sys.cancellation_requested()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("call should succeed");

    assert_eq!(result, BexExternalValue::Bool(false));
}

#[tokio::test]
async fn cancellation_requested_with_precancelled_token_returns_cancelled() {
    // With a pre-cancelled token, call_function fails fast before the VM
    // even starts. This guarantees consistent Err(Cancelled) regardless of
    // whether the function uses sync or async sys_ops.
    let source = r#"
        function main() -> bool {
            baml.sys.cancellation_requested()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
        .expect("Failed to create engine");

    let cancel = CancellationToken::new();
    cancel.cancel(); // Pre-cancel

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
        )
        .await;

    assert!(
        matches!(result, Err(EngineError::Cancelled)),
        "Pre-cancelled token should return Cancelled, got: {result:?}"
    );
}

// ============================================================================
// 6. Multiple sequential sleeps — cancel partway through
// ============================================================================

#[tokio::test]
async fn cancel_interrupts_sequential_sleeps() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(100);
            baml.sys.sleep(100);
            baml.sys.sleep(10000);
            baml.sys.sleep(10000);
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
            .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let start = std::time::Instant::now();

    let handle = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel_clone)
                        .build(),
                )
                .await
        }
    });

    // Cancel after the two short sleeps but during the long one.
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(EngineError::Cancelled)),
        "Expected EngineError::Cancelled, got: {result:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "Cancel took too long: {elapsed:?} (expected < 3s)"
    );
}

// ============================================================================
// 7. Non-cancelled token lets function complete normally
// ============================================================================

#[tokio::test]
async fn non_cancelled_token_completes_normally() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(50);
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("call should succeed");

    assert_eq!(result, BexExternalValue::Int(42));
}

// ============================================================================
// 8. Cancel is idempotent — multiple cancel() calls are harmless
// ============================================================================

#[tokio::test]
async fn cancel_is_idempotent() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(10000);
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_types::SysOps::native()))
            .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let handle = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel_clone)
                        .build(),
                )
                .await
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    cancel.cancel(); // second cancel — should be harmless
    cancel.cancel(); // third cancel — still harmless

    let result = handle.await.expect("task panicked");
    assert!(matches!(result, Err(EngineError::Cancelled)));
}
