//! Client option tests - ported from test_client_option_test.go
//!
//! Tests for client option functionality including:
//! - WithClient overriding
//! - Option precedence
//! - Combination with other options

use baml::ClientRegistry;
use baml::LogType;
use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;
use std::collections::HashMap;

/// Test with_client routes to correct client - Go: TestWithClientOption/WithClientRoutesToCorrectClient
#[test]
fn test_with_client_override() {
    // Go: Use WithClient to override the default client
    // Claude is defined in the BAML files
    let result = B.TestOpenAI.with_client("Claude").call("Say hello");
    assert!(
        result.is_ok(),
        "Expected successful call with Claude client, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");
}

/// Test with_client takes precedence over with_client_registry - Go: TestWithClientOption/WithClientTakesPrecedenceOverWithClientRegistry
#[test]
fn test_with_client_precedence_over_registry() {
    // Create a client registry with GPT4oMini as primary
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    registry.add_llm_client("MyGPT", "openai", options);
    registry.set_primary_client("MyGPT");

    // Go: WithClient should override the client registry's primary
    // Use Claude which is defined in BAML files
    let result = B
        .TestOpenAI
        .with_client_registry(&registry)
        .with_client("Claude")
        .call("Say hello");
    assert!(
        result.is_ok(),
        "Expected successful call with Claude overriding registry, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");
}

/// Test with_client_registry still works - Go: TestWithClientOption/WithClientRegistryStillWorks
#[test]
fn test_with_client_registry_still_works() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    registry.add_llm_client("MyGPT", "openai", options);
    registry.set_primary_client("MyGPT");

    let result = B
        .TestOpenAI
        .with_client_registry(&registry)
        .call("Say hello");
    assert!(
        result.is_ok(),
        "Expected successful call with registry, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");
}

/// Test client option with collector - Go: TestWithClientOption/WithClientAndCollector
#[test]
fn test_client_option_with_collector() {
    let collector = new_collector("client-option-test");

    let result = B
        .TestOpenAI
        .with_client("Claude")
        .with_collector(&collector)
        .call("Say hello");

    assert!(
        result.is_ok(),
        "Expected successful call with collector and client, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");

    // Go: Verify collector captured the call
    let logs = collector.logs();
    // Go: assert.Len(t, logs, 1)
    assert_eq!(logs.len(), 1, "Expected one log entry");

    // Verify the call was logged correctly
    let log = &logs[0];
    assert_eq!(
        log.function_name(),
        "TestOpenAI",
        "Function name should match"
    );
    assert_eq!(log.log_type(), LogType::Call, "Log type should be call");
}
