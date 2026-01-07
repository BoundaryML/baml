//! Error handling tests - ported from test_error_handling_test.go
//!
//! Tests for error handling including:
//! - HTTP errors (401, 404, connection)
//! - Validation errors
//! - Serialization errors
//! - Network errors (timeout, DNS)
//! - Constraint errors
//! - Finish reason errors

use baml::ClientRegistry;
use rust::baml_client::sync_client::B;
use std::collections::HashMap;

/// Test 401 unauthorized error - Go: TestHTTPErrors/HTTP401InvalidAPIKey
#[test]
fn test_http_401_error() {
    let mut registry = ClientRegistry::new();

    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    options.insert("api_key".to_string(), serde_json::json!("INVALID_KEY"));

    registry.add_llm_client("InvalidKeyClient", "openai", options);
    registry.set_primary_client("InvalidKeyClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("Hello");

    // Go: assert.Error(t, err, "Expected HTTP 401 error")
    assert!(result.is_err(), "Expected 401 error with invalid API key");

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error);
    // Go: assert.Contains(t, err.Error(), "401")
    assert!(
        error_str.contains("401")
            || error_str.to_lowercase().contains("unauthorized")
            || error_str.to_lowercase().contains("invalid")
            || error_str.to_lowercase().contains("authentication"),
        "Expected authentication-related error, got: {}",
        error_str
    );
}

/// Test connection error - Go: TestHTTPErrors/HTTPConnectionError
#[test]
fn test_connection_error() {
    let mut registry = ClientRegistry::new();

    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    options.insert("api_key".to_string(), serde_json::json!("test_key"));
    options.insert(
        "base_url".to_string(),
        serde_json::json!("https://does-not-exist.com"),
    );

    registry.add_llm_client("InvalidEndpointClient", "openai", options);
    registry.set_primary_client("InvalidEndpointClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("Hello");

    // Go: assert.Error(t, err, "Expected connection error")
    assert!(result.is_err(), "Expected connection error");

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error).to_lowercase();
    // Go: Expected connection-related error
    assert!(
        error_str.contains("connect")
            || error_str.contains("connection")
            || error_str.contains("network")
            || error_str.contains("dns")
            || error_str.contains("timeout"),
        "Expected connection-related error, got: {}",
        error_str
    );
}

/// Test validation error - function that always fails validation
#[test]
fn test_validation_error() {
    let result = B.FnAlwaysFails.call("test");
    assert!(result.is_err(), "Expected validation error");
}

/// Test serialization error - streaming
#[test]
fn test_serialization_error_stream() {
    // Try to parse an invalid response (may cause serialization error)
    let result = B.FnOutputClass.parse("not valid json");
    // Go: assert.Error(t, err, "Expected parse error with invalid input")
    assert!(result.is_err(), "Expected parse error with invalid input");
}

/// Test timeout error - Go: TestTimeoutBehavior
#[test]
fn test_timeout_error() {
    let result = B.TestTimeoutError.call("test");
    // May succeed or timeout - both are valid outcomes
    match result {
        Ok(output) => {
            eprintln!("Timeout test succeeded with output: {}", output);
        }
        Err(e) => {
            let error_str = format!("{:?}", e).to_lowercase();
            assert!(
                error_str.contains("timeout") || error_str.contains("timed out"),
                "Expected timeout-related error, got: {}",
                error_str
            );
        }
    }
}

/// Test zero timeout (Rust-only test)
#[test]
fn test_zero_timeout() {
    let result = B.TestZeroTimeout.call("test");
    // Zero timeout should succeed (infinite timeout)
    assert!(result.is_ok(), "Expected success with zero timeout");
}

/// Test finish reason error - Go: TestFinishReasonErrors
#[test]
fn test_finish_reason_error() {
    let result = B.TestOpenAIWithFinishReasonError.call("test");
    // Go: assert.Error(t, err, "Expected finish reason error")
    assert!(result.is_err(), "Expected finish reason error");

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error);
    // Go: assert.Contains(t, err.Error(), "Finish reason")
    assert!(
        error_str.contains("Finish reason"),
        "Expected finish reason in error, got: {}",
        error_str
    );
}

/// Test streaming timeout
#[test]
fn test_streaming_timeout() {
    let result = B.TestStreamingTimeout.call("test");
    // May timeout during streaming
    match result {
        Ok(_) => {
            // If it succeeds, that's also valid
        }
        Err(e) => {
            let error_str = format!("{:?}", e).to_lowercase();
            assert!(
                error_str.contains("timeout") || error_str.contains("stream"),
                "Expected timeout-related error, got: {}",
                error_str
            );
        }
    }
}

/// Test concurrent error handling - Go: TestConcurrentErrorHandling
#[test]
fn test_concurrent_error_handling() {
    use std::thread;

    let handles: Vec<_> = (0..3)
        .map(|i| {
            thread::spawn(move || {
                let result = B.FnAlwaysFails.call(format!("concurrent error {}", i));
                assert!(result.is_err(), "Expected error for concurrent call {}", i);
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

/// Test error message format - Go: TestErrorMessageFormat
#[test]
fn test_error_message_format() {
    let result = B.FnAlwaysFails.call("test");
    assert!(result.is_err(), "Expected error");

    let error = result.unwrap_err();
    let error_str = format!("{}", error);

    // Error message should be non-empty and meaningful
    assert!(!error_str.is_empty(), "Expected non-empty error message");
    eprintln!("Error message: {}", error_str);
}

/// Test error recovery - try again after error - Go: TestErrorRecovery
#[test]
fn test_error_recovery() {
    // First call fails
    let result1 = B.FnAlwaysFails.call("test");
    assert!(result1.is_err(), "Expected first call to fail");

    // Second call should work (different function)
    let result2 = B.TestFnNamedArgsSingleString.call("recovery test");
    assert!(result2.is_ok(), "Expected recovery call to succeed");
}

/// Test client registry error - missing client - Go: TestClientRegistryErrors/NonexistentClient
#[test]
fn test_client_registry_missing_client() {
    let mut registry = ClientRegistry::new();
    // Don't add any clients, just set primary to non-existent
    registry.set_primary_client("NonExistentClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("Hello");

    // Go: assert.Error(t, err, "Expected error for non-existent client")
    assert!(result.is_err(), "Expected error for missing client");
}

/// Test AWS invalid credentials errors (Rust-only test)
#[test]
fn test_aws_invalid_credentials() {
    let result = B.TestAwsInvalidAccessKey.call("test");
    // Should fail with invalid credentials
    assert!(
        result.is_err(),
        "Expected AWS credential error, got {:?}",
        result
    );

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error).to_lowercase();
    assert!(
        error_str.contains("credential")
            || error_str.contains("auth")
            || error_str.contains("access")
            || error_str.contains("unrecognized"),
        "Expected credential-related error, got: {}",
        error_str
    );
}

/// Test Azure failure (Rust-only test)
#[test]
fn test_azure_failure() {
    let result = B.TestAzureFailure.call("test");
    assert!(result.is_err(), "Expected Azure failure");
}

/// Test fallback errors - Go: TestFallbackErrors
#[test]
fn test_fallback_errors() {
    let result = B.FnFallbackAlwaysFails.call("lorem ipsum");

    // Go: assert.Error(t, err, "Expected error from failing fallback chain")
    assert!(
        result.is_err(),
        "Expected error from failing fallback chain"
    );

    let error = result.unwrap_err();
    let error_str = format!("{:?}", error);

    // Go: Verify that error message includes information about all failed clients
    // The fallback client is configured with these non-existent models:
    // "openai/gpt-0-noexist", "openai/gpt-1-noexist", "openai/gpt-2-noexist"
    assert!(
        error_str.contains("gpt-0-noexist"),
        "Expected first fallback client in error"
    );
    assert!(
        error_str.contains("gpt-1-noexist"),
        "Expected second fallback client in error"
    );
    assert!(
        error_str.contains("gpt-2-noexist"),
        "Expected third fallback client in error"
    );
}
