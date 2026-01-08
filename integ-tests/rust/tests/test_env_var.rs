//! Environment variable tests - ported from env_var_test.go
//!
//! Tests for environment variable handling including:
//! - Basic env var handling
//! - Overriding with options

use rust::baml_client::sync_client::B;

/// Test basic environment variable handling
#[test]
fn test_basic_env_var() {
    // Test that the function works with current env vars
    let result = B.TestOpenAI.call("Hello");
    assert!(result.is_ok(), "Expected successful call with env vars");
}

/// Test overriding env var with options
#[test]
fn test_override_env_var() {
    let result = B
        .TestFnNamedArgsSingleString
        .with_env_var("CUSTOM_VAR", "custom_value")
        .call("Test with custom env var");

    assert!(
        result.is_ok(),
        "Expected successful call with custom env var"
    );
}
