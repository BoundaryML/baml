//! Abort handler tests - ported from test_abort_handlers_test.go
//!
//! Tests for abort/cancellation functionality including:
//! - Manual cancellation
//! - Timeout cancellation
//! - Streaming cancellation
//! - Retry chain cancellation
//! - Fallback chain cancellation

use baml::CancellationToken;
use rust::baml_client::sync_client::B;
use std::thread;
use std::time::{Duration, Instant};

/// Test manual cancellation - Go: TestAbortHandlerManualCancellation
#[test]
fn test_manual_cancellation() {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    token.cancel();

    // This should be cancelled before completion
    let result = B.TestOpenAI.with_cancellation_token(Some(token));

    // Cancel after 100ms in another thread
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        token_clone.cancel();
    });
    let result = result.call("write a short story about a cat (100 words)");

    // Go: assert.Error(t, err) && assert.Contains(t, err.Error(), "context canceled")
    assert!(result.is_err(), "Expected error from cancellation");
    let error = result.unwrap_err();
    let error_str = format!("{:?}", error).to_lowercase();
    assert!(
        error_str.contains("cancel") || error_str.contains("abort"),
        "Expected cancellation error, got: {}",
        error_str
    );
}

/// Test timeout cancellation - Go: TestAbortHandlerTimeoutCancellation
#[test]
fn test_timeout_cancellation() {
    // Create token with 200ms timeout
    let token = CancellationToken::new_with_timeout(Duration::from_millis(200));

    // This should timeout before all retries complete
    let result = B
        .TestRetryExponential
        .with_cancellation_token(Some(token))
        .call();

    // Go: assert.Error(t, err) && assert.Contains(t, err.Error(), "deadline exceeded")
    assert!(result.is_err(), "Expected error from timeout");
    let error = result.unwrap_err();
    let error_str = format!("{:?}", error).to_lowercase();
    assert!(
        error_str.contains("cancel")
            || error_str.contains("timeout")
            || error_str.contains("deadline")
            || error_str.contains("aborterror"),
        "Expected timeout/cancellation error, got: {}",
        error_str
    );
}

/// Test streaming cancellation - Go: TestAbortHandlerStreamingCancellation
#[test]
fn test_streaming_cancellation() {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let stream = B
        .TestFallbackClient
        .with_cancellation_token(Some(token))
        .stream();

    assert!(stream.is_ok(), "Expected successful stream creation");
    let mut stream = stream.unwrap();

    // Cancel after 50ms in another thread
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        token_clone.cancel();
    });

    let mut count = 0;
    for partial in stream.partials() {
        if partial.is_ok() {
            count += 1;
        }
    }

    // Go: assert.Less(t, count, 10, "Stream should have been cancelled early")
    // Stream should have stopped early due to cancellation
    assert!(
        count < 10,
        "Stream should have been cancelled early, got {} partials",
        count
    );
}

/// Test retry chain cancellation - Go: TestAbortHandlerRetryChainCancellation
#[test]
fn test_retry_chain_cancellation() {
    // guarantee all resources are initialized ahead of time, so we don't measure construction time
    rust::baml_client::init();

    // Create token with 300ms timeout
    let token = CancellationToken::new_with_timeout(Duration::from_millis(300));

    let result = B.TestRetryExponential.with_cancellation_token(Some(token));

    let start = Instant::now();
    let result = result.call();
    let duration = start.elapsed();

    // Go: assert.Error(t, err)
    assert!(result.is_err(), "Expected error from cancellation");

    // Go: assert.Less(t, duration, 400*time.Millisecond, "Should have cancelled before all retries")
    // Should have been cancelled before all exponential retries complete
    // Exponential delays would sum up to more than 300ms
    assert!(
        duration < Duration::from_millis(400),
        "Should have cancelled before all retries, took {:?}",
        duration
    );
}

/// Test fallback chain cancellation - Go: TestAbortHandlerFallbackChainCancellation
#[test]
fn test_fallback_chain_cancellation() {
    // guarantee all resources are initialized ahead of time, so we don't measure construction time
    rust::baml_client::init();

    // Create token with 150ms timeout
    let token = CancellationToken::new_with_timeout(Duration::from_millis(150));

    let start = Instant::now();
    let result = B
        .TestFallbackClient
        .with_cancellation_token(Some(token))
        .call();
    let duration = start.elapsed();

    // Go: assert.Error(t, err)
    assert!(result.is_err(), "Expected error from cancellation");

    // Go: assert.Less(t, duration, 200*time.Millisecond, "Should have cancelled during fallback chain")
    assert!(
        duration < Duration::from_millis(200),
        "Should have cancelled during fallback chain, took {:?}",
        duration
    );
}

/// Test no interference with normal operation - Go: TestAbortHandlerNoInterferenceWithNormalOperation
#[test]
fn test_no_interference_normal_operation() {
    // Test that operations complete normally when not cancelled
    let result = B.ExtractNames.call("My name is John Doe");

    // Should complete successfully (or fail due to LLM, but not due to cancellation)
    match result {
        Ok(names) => {
            assert!(!names.is_empty(), "Expected non-empty result");
        }
        Err(e) => {
            let error_str = format!("{:?}", e).to_lowercase();
            // Go: We're just checking that cancellation doesn't interfere when not triggered
            assert!(
                !error_str.contains("cancel") && !error_str.contains("deadline"),
                "Should not have cancellation error, got: {}",
                error_str
            );
        }
    }
}

/// Test multiple concurrent cancellations - Go: TestAbortHandlerMultipleConcurrentCancellations
#[test]
fn test_multiple_concurrent_cancellations() {
    let token = CancellationToken::new();
    let token_for_cancel = token.clone();

    let handles: Vec<_> = (0..3)
        .map(|_| {
            let t = token.clone();
            thread::spawn(move || B.TestRetryConstant.with_cancellation_token(Some(t)).call())
        })
        .collect();

    // Cancel all operations after 100ms
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        token_for_cancel.cancel();
    });

    // Collect all results
    let mut error_count = 0;
    for handle in handles {
        let result = handle.join().expect("Thread panicked");
        if result.is_err() {
            error_count += 1;
        }
    }

    // Go: All operations should have been cancelled
    // When aborted, you should get the same error as the context
    assert_eq!(error_count, 3, "Expected all 3 operations to be cancelled");
}
