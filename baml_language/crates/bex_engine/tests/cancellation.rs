//! Tests for cancellation support in the BEX engine.
//!
//! Verifies that `CancellationToken` correctly interrupts function execution
//! at various points: immediately, during sleep, during HTTP, and across
//! retry/fallback orchestration strategies.

mod common;

use std::sync::Arc;

use bex_engine::{
    BexEngine, BexExternalValue, CANCELLED_PANIC_CLASS, CancellationToken, EngineError,
    FunctionCallContextBuilder,
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Asserts the result is an unhandled `baml.panics.Cancelled` panic.
#[track_caller]
fn assert_cancelled<T: std::fmt::Debug>(result: &Result<T, EngineError>) {
    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance { class_name, .. } => {
                assert_eq!(
                    class_name, CANCELLED_PANIC_CLASS,
                    "expected {CANCELLED_PANIC_CLASS} panic, got class {class_name}"
                );
            }
            other => panic!("expected panic Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow({CANCELLED_PANIC_CLASS}), got {other:?}"),
    }
}

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
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    cancel.cancel(); // Cancel before the call

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true,
        )
        .await;

    assert_cancelled(&result);
}

// ============================================================================
// 2. Cancellation during sleep — engine should exit promptly
// ============================================================================

#[tokio::test]
async fn cancel_during_sleep_returns_promptly() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
                )
                .await
        }
    });

    // Give the function time to start the sleep, then cancel.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert_cancelled(&result);
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
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
                )
                .await
        }
    });

    // Give the function time to start the HTTP request, then cancel.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert_cancelled(&result);
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
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
            1
        }

        function fast() -> int {
            2
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
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
                    true,
                )
                .await
        }
    });

    // Cancel only the slow call.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel_slow.cancel();

    let result_slow = handle_slow.await.expect("task panicked");
    let result_fast = handle_fast.await.expect("task panicked");

    assert_cancelled(&result_slow);
    assert_eq!(
        result_fast.expect("fast call failed"),
        BexExternalValue::Int(2),
        "Fast call should complete normally"
    );
}

// ============================================================================
// 5. Multiple sequential sleeps — cancel partway through
// ============================================================================

#[tokio::test]
async fn cancel_interrupts_sequential_sleeps() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
                )
                .await
        }
    });

    // Cancel after the two short sleeps but during the long one.
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    cancel.cancel();

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert_cancelled(&result);
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "Cancel took too long: {elapsed:?} (expected < 3s)"
    );
}

// ============================================================================
// 6. Non-cancelled token lets function complete normally
// ============================================================================

#[tokio::test]
async fn non_cancelled_token_completes_normally() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed");

    assert_eq!(result, BexExternalValue::Int(42));
}

// ============================================================================
// 7. Cancel is idempotent — multiple cancel() calls are harmless
// ============================================================================

#[tokio::test]
async fn cancel_is_idempotent() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
                )
                .await
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    cancel.cancel(); // second cancel — should be harmless
    cancel.cancel(); // third cancel — still harmless

    let result = handle.await.expect("task panicked");
    assert_cancelled(&result);
}

// ============================================================================
// 8. M1 — `cancel_function_call(call_id)` actually cancels a running call.
// ============================================================================

#[tokio::test]
async fn cancel_function_call_by_id_actually_cancels() {
    // The host calls `engine.cancel_function_call(call_id)` instead of firing
    // a `CancellationToken` directly. This exercises the `active_calls` /
    // `ActiveCallGuard` registration path. Pre-fix: the registry was empty
    // and `cancel_function_call` always returned `FunctionCallNotFound`,
    // leaving the call to run to completion (10s sleep). Post-fix: the
    // registered cancel token fires and the call returns Cancelled within
    // ~1s.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let call_id = sys_types::CallId::next();
    let start = std::time::Instant::now();

    let handle = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(call_id).build(),
                    true,
                )
                .await
        }
    });

    // Let the function reach the sleep, then cancel by id.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    engine
        .cancel_function_call(call_id)
        .expect("call_id should be registered while the call is in flight");

    let result = handle.await.expect("task panicked");
    let elapsed = start.elapsed();

    assert_cancelled(&result);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "cancel_function_call took too long: {elapsed:?} (expected < 2s)"
    );

    // After completion, the entry is gone; a second cancel reserves the same
    // ID as already-cancelled for any future call that tries to use it.
    assert!(engine.cancel_function_call(call_id).is_ok());
}

#[tokio::test]
async fn cancel_function_call_by_id_before_registration_pre_cancels() {
    let source = r#"
        function main() -> int {
            7
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let call_id = sys_types::CallId::next();
    engine
        .cancel_function_call(call_id)
        .expect("unknown call_id should be reserved as pre-cancelled");

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(call_id).build(),
            true,
        )
        .await;

    assert_cancelled(&result);
}

#[tokio::test]
async fn shutdown_ignores_pre_cancelled_call_that_never_started() {
    let source = r#"
        function main() -> int {
            7
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    engine
        .cancel_function_call(sys_types::CallId::next())
        .expect("unknown call_id should be reserved as pre-cancelled");

    tokio::time::timeout(std::time::Duration::from_secs(1), engine.shutdown())
        .await
        .expect("shutdown should not wait for a call that never started");
}

#[tokio::test]
async fn duplicate_call_id_is_rejected() {
    // Two concurrent calls with the same `CallId` should fail-fast on the
    // second. The first holds the registry slot via `ActiveCallGuard`.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(500n));
            7
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let shared_id = sys_types::CallId::next();
    let first = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(shared_id).build(),
                    true,
                )
                .await
        }
    });

    // Give the first call time to register, then start the duplicate.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let second_result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(shared_id).build(),
            true,
        )
        .await;
    assert!(
        matches!(second_result, Err(EngineError::DuplicateCallId { .. })),
        "expected DuplicateCallId, got {second_result:?}"
    );

    // First call should still complete normally.
    let first_result = first.await.expect("task panicked");
    assert_eq!(
        first_result.expect("first call failed"),
        BexExternalValue::Int(7)
    );
}

// ============================================================================
// 9. M3 — engine-synthesized vs VM-thrown Cancelled panic shape equivalence.
// ============================================================================

#[tokio::test]
async fn cancelled_panic_shape_equivalence() {
    // The engine produces two cancellation paths that must be observationally
    // equivalent for host bridges:
    //   (a) `cancelled_unhandled_throw()` synthesizes one when the engine
    //       short-circuits before the VM runs (pre-call check; "cancel wins"
    //       after Complete).
    //   (b) The VM's `Await` opcode throws `baml.panics.Cancelled` when the
    //       awaited future is in `Cancelled` state.
    //
    // Both must produce the same `class_name` and the same `message` field
    // so downstream `is_cancelled_engine_error` works uniformly.
    use bex_engine::cancelled_unhandled_throw;
    use indexmap::IndexMap;

    fn extract_instance(v: &BexExternalValue) -> (&str, &IndexMap<String, BexExternalValue>) {
        match v {
            BexExternalValue::Instance {
                class_name, fields, ..
            } => (class_name.as_str(), fields),
            other => panic!("expected Instance, got {other:?}"),
        }
    }

    let synthesized = cancelled_unhandled_throw();
    let synthesized_value = match synthesized {
        EngineError::UnhandledThrow { value, .. } => *value,
        other => panic!("cancelled_unhandled_throw returned non-UnhandledThrow: {other:?}"),
    };

    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
            42
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
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
                    true,
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    let vm_result = handle.await.expect("task panicked");
    let vm_value = match vm_result {
        Err(EngineError::UnhandledThrow { value, .. }) => *value,
        other => panic!("expected UnhandledThrow from VM Await, got {other:?}"),
    };

    let (s_class, s_fields) = extract_instance(&synthesized_value);
    let (v_class, v_fields) = extract_instance(&vm_value);

    assert_eq!(s_class, CANCELLED_PANIC_CLASS);
    assert_eq!(v_class, CANCELLED_PANIC_CLASS);
    assert_eq!(s_fields.len(), v_fields.len(), "field count must match");
    for (s_key, s_val) in s_fields {
        let v_val = v_fields
            .get(s_key)
            .unwrap_or_else(|| panic!("VM panic missing field {s_key}"));
        assert_eq!(s_val, v_val, "field {s_key} differs");
    }
}

// ============================================================================
// BEP-034 spawn options — `spawn with baml.spawn.options(cancel = tok)`
// ============================================================================

/// Firing a BAML-surface `CancelToken` passed via
/// `baml.spawn.options(cancel = ...)` cancels the spawned child; awaiting it
/// re-throws `Cancelled`, and it returns well before the 10s sleep would.
#[tokio::test]
async fn spawn_with_options_cancel_token_propagates() {
    let source = r#"
        function main() -> int {
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                42
            };
            let _ = tok.cancel();
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let start = std::time::Instant::now();
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_cancelled(&result);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "cancel via options(cancel=) took too long: {:?}",
        start.elapsed()
    );
}

/// `Cancelled` delivered to a token-cancelled spawn is catchable at the
/// awaiter — the `catch` arm runs and produces the fallback value.
#[tokio::test]
async fn spawn_with_options_cancel_token_is_catchable() {
    let source = r#"
        function main() -> int {
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                42
            };
            let _ = tok.cancel();
            (await f) catch (e) { baml.panics.Cancelled => 0 }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed (Cancelled caught)");

    assert_eq!(result, BexExternalValue::Int(0));
}

/// A spawn configured with `options(cancel = tok)` whose token is never fired
/// completes normally — the option must not perturb the happy path.
#[tokio::test]
async fn spawn_with_options_uncancelled_completes_normally() {
    let source = r#"
        function main() -> int {
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok) { 42 };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed");

    assert_eq!(result, BexExternalValue::Int(42));
}

/// `CancelToken.any([a, b])` composes inputs: firing one input cancels a spawn
/// configured with the composite token. The awaiter catches `Cancelled` and
/// returns the fallback; if composition were broken the 10s sleep would run to
/// completion and yield 42 instead.
#[tokio::test]
async fn spawn_with_options_cancel_token_any_composes() {
    let source = r#"
        function main() -> int {
            let a = baml.spawn.CancelToken.new();
            let b = baml.spawn.CancelToken.new();
            let combined = baml.spawn.CancelToken.any([a, b]);
            let f = spawn with baml.spawn.options(cancel = combined) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                42
            };
            let _ = a.cancel();
            (await f) catch (e) { baml.panics.Cancelled => 0 }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let start = std::time::Instant::now();
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed (Cancelled caught)");

    assert_eq!(result, BexExternalValue::Int(0));
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "any-composed cancel took too long: {:?}",
        start.elapsed()
    );
}

// ============================================================================
// BEP-034 spawn options — `detach = true`
// ============================================================================

/// `detach = true` does not perturb normal completion.
#[tokio::test]
async fn spawn_with_options_detach_completes_normally() {
    let source = r#"
        function main() -> int {
            let f = spawn with baml.spawn.options(detach = true) { 42 };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed");

    assert_eq!(result, BexExternalValue::Int(42));
}

/// A `detach = true` spawn still honors an explicit `cancel` token: detach only
/// drops the *parent* token from the effective token, so a user token linked
/// via `options(cancel = ...)` still cancels it.
#[tokio::test]
async fn spawn_with_options_detach_still_honors_cancel_token() {
    let source = r#"
        function main() -> int {
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok, detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                42
            };
            let _ = tok.cancel();
            (await f) catch (e) { baml.panics.Cancelled => 0 }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let start = std::time::Instant::now();
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed (Cancelled caught)");

    assert_eq!(result, BexExternalValue::Int(0));
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "detached cancel-token took too long: {:?}",
        start.elapsed()
    );
}

/// `spawn ... with` accepts only a `baml.spawn.options(...)` config: any other
/// expression is a compile error (BEP-034 spawn options, v1). `compile_for_engine`
/// panics on diagnostics, so a rejected program unwinds here.
#[test]
fn spawn_with_non_options_is_rejected() {
    let bad = r#"
        function helper() -> int { 1 }
        function main() -> int {
            let f = spawn with helper() { 1 };
            await f
        }
    "#;
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // suppress the diagnostic backtrace
    let compiled = std::panic::catch_unwind(|| compile_for_engine(bad));
    std::panic::set_hook(prev);
    assert!(
        compiled.is_err(),
        "`spawn with helper()` should fail to compile (with accepts only baml.spawn.options(...))"
    );
}
