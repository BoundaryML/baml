//! Retries and fallbacks tests - ported from test_retries_fallbacks_test.go
//!
//! Tests for retry and fallback functionality including:
//! - Exponential backoff
//! - Fallback chains
//! - Failure handling
//! - Timeout behavior
//! - Streaming with retries

use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;
use std::time::{Duration, Instant};

/// Test exponential backoff retries - Go: TestRetryExponential
/// CRITICAL FIX: Go expects error after retry exhaustion, not success!
#[test]
fn test_exponential_backoff() {
    let start = Instant::now();

    let result = B.TestRetryExponential.call();

    let elapsed = start.elapsed();

    // Go: assert.Error(t, err, "Expected an exception but none was raised")
    assert!(
        result.is_err(),
        "Expected error after retry exhaustion, got {:?}",
        result
    );

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error).to_lowercase();
    // Go: error should indicate retry exhaustion or similar
    assert!(
        error_str.contains("retry")
            || error_str.contains("timeout")
            || error_str.contains("failed")
            || error_str.contains("exhausted"),
        "Expected retry-related error message, got: {}",
        error_str
    );

    // Go timing thresholds
    assert!(
        elapsed > Duration::from_millis(100),
        "Expected retry delay > 100ms, took {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "Expected retry to complete within 60s, took {:?}",
        elapsed
    );

    eprintln!("Exponential backoff test completed in {:?}", elapsed);
}

/// Test constant delay retries
#[test]
fn test_constant_delay_retries() {
    let start = Instant::now();

    let result = B.TestRetryConstant.call();

    let elapsed = start.elapsed();
    assert!(result.is_err(), "Expected error call, got {:?}", result);
    eprintln!("Constant delay test completed in {:?}", elapsed);
}

/// Test fallback chain - Go: TestFallbackChains
#[test]
fn test_fallback_chain() {
    let result = B.TestFallbackClient.call();
    assert!(
        result.is_ok(),
        "Expected successful fallback, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result but got empty")
    assert!(!output.is_empty(), "Expected non-empty result");
}

/// Test single fallback - Go: TestFailureHandling
/// CRITICAL FIX: Go expects error with ConnectError!
#[test]
fn test_single_fallback() {
    let result = B.TestSingleFallbackClient.call();
    assert!(
        result.is_err(),
        "Expected error from single fallback client, got {:?}",
        result
    );

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error);
    // Go: assert.Contains(t, err.Error(), "ConnectError")
    assert!(
        error_str.contains("ConnectError"),
        "Expected ConnectError in error message, got: {}",
        error_str
    );
}

/// Test failure handling - function that always fails
#[test]
fn test_failure_handling() {
    let result = B.FnAlwaysFails.call("test");
    assert!(result.is_err(), "Expected error from FnAlwaysFails");
}

/// Test fallback always fails
#[test]
fn test_fallback_always_fails() {
    let result = B.FnFallbackAlwaysFails.call("test");
    assert!(result.is_err(), "Expected error when all fallbacks fail");
}

/// Test fallback strategy - Go: TestFallbackStrategies
#[test]
fn test_fallback_strategy() {
    let result = B.TestFallbackStrategy.call("Dr. Pepper");
    assert!(
        result.is_ok(),
        "Expected successful fallback strategy, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from fallback strategy")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from fallback strategy"
    );
}

/// Test retries with different providers
#[test]
fn test_retries_with_different_providers() {
    // Test OpenAI
    let result = B.TestOpenAI.call("Hello");
    assert!(result.is_ok(), "Expected successful OpenAI call");

    // Test Anthropic
    let result = B.TestAnthropic.call("Hello");
    assert!(result.is_ok(), "Expected successful Anthropic call");
}

/// Test timeout behavior - Go: TestTimeoutBehavior
#[test]
fn test_timeout_behavior() {
    let result = B.TestTimeoutFallback.call("test");

    // Should either succeed with fallback or fail with timeout
    match result {
        Ok(output) => {
            assert!(
                !output.is_empty(),
                "Expected non-empty output from fallback"
            );
        }
        Err(e) => {
            let error_str = format!("{:?}", e).to_lowercase();
            assert!(
                error_str.contains("timeout") || error_str.contains("fallback"),
                "Expected timeout or fallback error, got: {}",
                error_str
            );
        }
    }
}

/// Test retries with streaming - Go: TestRetryWithStreaming
#[test]
fn test_retries_with_streaming() {
    let stream = B.TestFallbackToShorthand.stream("Mt Rainier is tall");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut had_error = false;

    for partial in stream.partials() {
        if partial.is_err() {
            had_error = true;
        }
    }

    let result = stream.get_final_response();
    if !had_error {
        // If no errors during streaming, should have final result
        assert!(
            result.is_ok(),
            "Expected successful final result from streaming fallback"
        );
        if let Ok(value) = result {
            assert!(!value.is_empty(), "Expected non-empty final result");
        }
    }
}

/// Test retry backoff timing - Go: TestRetryBackoffTiming
#[test]
fn test_retry_backoff_timing() {
    let start = Instant::now();

    let result = B.TestRetryExponential.call();

    let elapsed = start.elapsed();

    // Go: assert.Error(t, err, "Expected error from retry exponential")
    assert!(
        result.is_err(),
        "Expected error from retry exponential, got {:?}",
        result
    );

    // Go timing thresholds
    assert!(
        elapsed > Duration::from_millis(100),
        "Expected some delay due to retry backoff, took: {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "Expected retry to complete within reasonable time, took: {:?}",
        elapsed
    );

    eprintln!("Backoff timing test: {:?} elapsed", elapsed);
}

/// Test concurrent retries and fallbacks - Go: TestConcurrentRetriesAndFallbacks
#[test]
fn test_concurrent_retries_fallbacks() {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    const NUM_CONCURRENT: usize = 3;
    let (tx, rx) = mpsc::channel();

    // Start multiple operations that might use fallbacks concurrently
    for i in 0..NUM_CONCURRENT {
        let tx = tx.clone();
        thread::spawn(move || {
            let result = B.TestFallbackClient.call();
            tx.send((i, result)).unwrap();
        });
    }
    drop(tx);

    let mut success_count = 0;
    let mut error_count = 0;

    // Collect results with timeout
    for _ in 0..NUM_CONCURRENT {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok((_, Ok(result))) => {
                assert!(!result.is_empty());
                success_count += 1;
            }
            Ok((_, Err(e))) => {
                eprintln!("Concurrent fallback error: {:?}", e);
                error_count += 1;
            }
            Err(_) => panic!("Timeout waiting for concurrent operations"),
        }
    }

    // Go: assert.Greater(t, successCount, 0, "Expected at least some concurrent operations to succeed")
    assert!(
        success_count > 0,
        "Expected at least some concurrent operations to succeed"
    );

    eprintln!(
        "Concurrent operations: {} succeeded, {} failed",
        success_count, error_count
    );
}

/// Test fallback with collector - Go: TestFallbackWithCollector
#[test]
fn test_fallback_with_collector() {
    let collector = new_collector("fallback-collector");

    let result = B.TestFallbackClient.with_collector(&collector).call();

    assert!(
        result.is_ok(),
        "Expected successful fallback with collector, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(!output.is_empty());

    // Go: Verify collector captured the fallback behavior
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Expected one log entry");

    let log = &logs[0];
    assert_eq!(
        log.function_name(),
        "TestFallbackClient",
        "Expected correct function name"
    );

    // Go: Check if multiple calls were made (indicating fallback attempts)
    let calls = log.calls();
    assert!(!calls.is_empty(), "Expected at least one call");

    // Go: Verify final call was successful
    let final_call = &calls[calls.len() - 1];
    assert!(
        final_call.selected(),
        "Expected final call to be selected/successful"
    );
}

/// Test round robin strategy - Go: TestFallbackStrategies (RoundRobinStrategy sub-test)
#[test]
fn test_round_robin_strategy() {
    let result = B.TestRoundRobinStrategy.call("Dr. Pepper");
    assert!(
        result.is_ok(),
        "Expected successful round robin call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from round robin strategy")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from round robin strategy"
    );
}
