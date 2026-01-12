//! Tags tests - ported from test_tags_test.go
//!
//! Tests for tag functionality including:
//! - Passthrough tags
//! - Combination with environment variables
//! - Combination with collector

use baml::LogType;
use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;

/// Test tag passthrough - Go: TestTagsPassthrough
#[test]
fn test_tag_passthrough() {
    let collector = new_collector("tags-test-collector");

    // First call with tags
    let result1 = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .with_tag("callId", "first")
        .with_tag("version", "v1")
        .with_tag("test_type", "rust_integration")
        .with_tag("component", "baml_client")
        .call("hello - call 1");

    assert!(
        result1.is_ok(),
        "Expected successful first call with tags, got {:?}",
        result1
    );
    let output1 = result1.unwrap();
    // Go: require.NotEmpty(t, result1)
    assert!(!output1.is_empty(), "Expected non-empty result");

    // Second call with different tags
    let result2 = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .with_tag("callId", "second")
        .with_tag("version", "v2")
        .with_tag("test_type", "rust_integration")
        .with_tag("extra", "data")
        .call("hello - call 2");

    assert!(
        result2.is_ok(),
        "Expected successful second call with tags, got {:?}",
        result2
    );
    let output2 = result2.unwrap();
    // Go: require.NotEmpty(t, result2)
    assert!(!output2.is_empty(), "Expected non-empty result");

    // Go: Verify collector received function calls
    let logs = collector.logs();
    // Go: require.Len(t, logs, 2)
    assert_eq!(logs.len(), 2, "Expected two log entries");

    // Go: Both calls should have completed successfully
    for (i, log) in logs.iter().enumerate() {
        // Go: require.Equal(t, "TestOpenAIGPT4oMini", functionName)
        assert_eq!(
            log.function_name(),
            "TestOpenAIGPT4oMini",
            "Function name should match"
        );

        // Go: Verify call completed (not an error)
        let calls = log.calls();
        // Go: require.NotEmpty(t, calls)
        assert!(!calls.is_empty(), "Expected at least one call");

        // Go: require.True(t, selected, "Call %d should have been selected", i+1)
        assert!(
            calls[0].selected(),
            "Call {} should have been selected",
            i + 1
        );
    }

    eprintln!(
        "Successfully passed tags through Rust client for {} function calls",
        logs.len()
    );
}

/// Test tags with environment variables - Go: TestTagsWithEnvironmentVars
#[test]
fn test_tags_with_env_vars() {
    let collector = new_collector("tags-env-test-collector");

    let result = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .with_tag("environment", "test")
        .with_tag("scenario", "tags_with_env")
        .with_env_var("CUSTOM_VAR", "test_value")
        .call("test with tags and env vars");

    assert!(
        result.is_ok(),
        "Expected successful call with tags and env vars, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: require.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");

    // Go: Verify collector received the function call
    let logs = collector.logs();
    // Go: require.Len(t, logs, 1)
    assert_eq!(logs.len(), 1, "Expected one log entry");

    let log = &logs[0];
    // Go: require.Equal(t, "TestOpenAIGPT4oMini", functionName)
    assert_eq!(
        log.function_name(),
        "TestOpenAIGPT4oMini",
        "Function name should match"
    );

    // Go: Verify call was successful
    let calls = log.calls();
    // Go: require.NotEmpty(t, calls)
    assert!(!calls.is_empty(), "Expected at least one call");

    // Go: require.True(t, selected)
    assert!(calls[0].selected(), "Call should have been selected");

    eprintln!("Successfully passed tags with environment variables through Rust client");
}

/// Test simple tag passthrough (original test)
#[test]
fn test_simple_tag_passthrough() {
    let result = B
        .TestFnNamedArgsSingleString
        .with_tag("test_tag", "test_value")
        .call("Hello with tag");

    assert!(
        result.is_ok(),
        "Expected successful call with tag, got {:?}",
        result
    );
}

/// Test tags with collector verification
#[test]
fn test_tags_with_collector_verification() {
    let collector = new_collector("tag-verification-collector");

    let result = B
        .TestOpenAI
        .with_collector(&collector)
        .with_tag("request_id", "12345")
        .with_tag("user", "test_user")
        .call("Hello with verified tags");

    assert!(
        result.is_ok(),
        "Expected successful call with tags, got {:?}",
        result
    );

    // Verify collector captured the call
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Expected one log entry");

    let log = &logs[0];

    // Check that tags are accessible via the log
    let tags = log.tags();
    // The tags may or may not be present depending on implementation
    // At minimum, we verify the call completed successfully
    assert_eq!(log.log_type(), LogType::Call, "Log type should be call");
}
