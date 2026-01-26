//! Client registry tests - ported from test_client_registry_test.go
//!
//! Tests for ClientRegistry functionality including:
//! - Dynamic client creation
//! - Vertex AI with JSON credentials
//! - Provider switching
//! - Validation
//! - Custom configurations
//! - Combined with collector

use baml::ClientRegistry;
use baml::LogType;
use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;
use std::collections::HashMap;

/// Test dynamic client creation - Go: TestDynamicClientCreation
#[test]
fn test_dynamic_client_creation() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-3.5-turbo"));
    registry.add_llm_client("MyClient", "openai", options);
    registry.set_primary_client("MyClient");

    let result = B.ExpectFailure.with_client_registry(&registry).call();

    assert!(
        result.is_ok(),
        "Expected successful call with dynamic client, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: assert.Contains(t, lowerResult, "london")
    assert!(
        output.contains("london"),
        "Expected output to contain 'london', got: {}",
        output
    );
}

/// Test Vertex AI with JSON string credentials - Go: TestClientRegistryVertexAIWithJSONCredentials
#[test]
fn test_vertex_ai_json_string_credentials() {
    let credentials =
        std::env::var("INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT").unwrap_or_default();

    if credentials.is_empty() {
        eprintln!(
            "Skipping Vertex AI test - INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT not set"
        );
        return;
    }

    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gemini-2.5-flash"));
    options.insert("location".to_string(), serde_json::json!("us-central1"));
    options.insert("credentials".to_string(), serde_json::json!(credentials));
    registry.add_llm_client("MyClient", "vertex-ai", options);
    registry.set_primary_client("MyClient");

    let result = B.ExpectFailure.with_client_registry(&registry).call();

    // Go: require.NoError(t, err) and assert.Contains(t, result, "london")
    assert!(
        result.is_ok(),
        "Expected successful Vertex AI call, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(
        output.contains("london"),
        "Expected output to contain 'london', got: {}",
        output
    );
}

/// Test Vertex AI with JSON object credentials - Go: TestClientRegistryVertexAIWithJSONObjectCredentials
#[test]
fn test_vertex_ai_json_object_credentials() {
    let credentials_str =
        std::env::var("INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT").unwrap_or_default();

    if credentials_str.is_empty() {
        eprintln!("Skipping Vertex AI object test - no credentials");
        return;
    }

    // Parse credentials as JSON object
    let creds_obj: serde_json::Value =
        serde_json::from_str(&credentials_str).unwrap_or(serde_json::json!({}));

    if creds_obj.is_null() || creds_obj.as_object().map_or(true, |o| o.is_empty()) {
        eprintln!("Skipping Vertex AI object test - could not parse credentials");
        return;
    }

    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gemini-2.5-flash"));
    options.insert("location".to_string(), serde_json::json!("us-central1"));
    options.insert("credentials".to_string(), creds_obj);
    registry.add_llm_client("MyClient", "vertex-ai", options);
    registry.set_primary_client("MyClient");

    let result = B.ExpectFailure.with_client_registry(&registry).call();

    // Go: require.NoError(t, err) and assert.Contains(t, result, "london")
    assert!(
        result.is_ok(),
        "Expected successful Vertex AI call, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(
        output.contains("london"),
        "Expected output to contain 'london', got: {}",
        output
    );
}

/// Test provider switching - Go: TestClientRegistryProviderSwitching
#[test]
fn test_provider_switching() {
    let mut registry = ClientRegistry::new();

    // Add OpenAI client
    let mut openai_options = HashMap::new();
    openai_options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    registry.add_llm_client("OpenAIClient", "openai", openai_options);

    // Test with OpenAI
    registry.set_primary_client("OpenAIClient");
    let result = B.TestOpenAI.with_client_registry(&registry).call("test");
    assert!(
        result.is_ok(),
        "Expected successful OpenAI call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from %s", p.provider)
    assert!(!output.is_empty(), "Expected non-empty result from OpenAI");
}

/// Test client registry validation - Go: TestClientRegistryValidation
#[test]
fn test_client_registry_validation_nonexistent() {
    let mut registry = ClientRegistry::new();
    // Try to set non-existent client as primary
    registry.set_primary_client("DoesNotExist");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected error when using non-existent client")
    assert!(
        result.is_err(),
        "Expected error when using non-existent client"
    );
}

/// Test client registry validation with invalid provider - Go: TestClientRegistryValidation/InvalidProviderType
#[test]
fn test_client_registry_validation_invalid_provider() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("some-model"));
    registry.add_llm_client("InvalidClient", "invalid-provider", options);
    registry.set_primary_client("InvalidClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected error when adding client with invalid provider")
    assert!(
        result.is_err(),
        "Expected error when using invalid provider"
    );
}

/// Test custom base URL - Go: TestClientRegistryWithCustomConfigs/CustomBaseURL
#[test]
fn test_custom_base_url() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    options.insert(
        "base_url".to_string(),
        serde_json::json!("https://does-not-exist.com"),
    );
    registry.add_llm_client("CustomClient", "openai", options);
    registry.set_primary_client("CustomClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected connection error with invalid base URL")
    assert!(
        result.is_err(),
        "Expected connection error with invalid base URL"
    );
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "ConnectError")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("ConnectError"),
        "Expected 'ConnectError' in error, got: {}",
        error_str
    );
}

/// Test invalid API key - Go: TestClientRegistryWithCustomConfigs/InvalidAPIKey
#[test]
fn test_invalid_api_key() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    options.insert("api_key".to_string(), serde_json::json!("INVALID_KEY"));
    registry.add_llm_client("InvalidKeyClient", "openai", options);
    registry.set_primary_client("InvalidKeyClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected authentication error with invalid API key")
    assert!(
        result.is_err(),
        "Expected authentication error with invalid API key"
    );
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "401")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("401"),
        "Expected '401' in error, got: {}",
        error_str
    );
}

/// Test invalid model - Go: TestClientRegistryWithCustomConfigs/InvalidModel
#[test]
fn test_invalid_model() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert(
        "model".to_string(),
        serde_json::json!("random-model-that-does-not-exist"),
    );
    registry.add_llm_client("InvalidModelClient", "openai", options);
    registry.set_primary_client("InvalidModelClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected model not found error")
    assert!(result.is_err(), "Expected model not found error");
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "404")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("404"),
        "Expected '404' in error, got: {}",
        error_str
    );
}

/// Test client registry with collector - Go: TestClientRegistryWithCollector
#[test]
fn test_client_registry_with_collector() {
    let mut registry = ClientRegistry::new();
    let collector = new_collector("registry-collector");

    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    registry.add_llm_client("CustomGPT", "openai", options);
    registry.set_primary_client("CustomGPT");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .with_collector(&collector)
        .call("test with custom client");

    assert!(
        result.is_ok(),
        "Expected successful call with custom client, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result)
    assert!(!output.is_empty(), "Expected non-empty result");

    // Go: Verify collector captured the call with custom client
    let logs = collector.logs();
    // Go: assert.Len(t, logs, 1)
    assert_eq!(logs.len(), 1, "Expected one log entry");

    let log = &logs[0];
    let calls = log.calls();
    // Go: assert.Len(t, calls, 1)
    assert_eq!(calls.len(), 1, "Expected one call");

    let call = &calls[0];
    // Go: assert.Equal(t, "CustomGPT", name)
    assert_eq!(
        call.client_name(),
        "CustomGPT",
        "Client name should be CustomGPT"
    );
    // Go: assert.Equal(t, "openai", provider)
    assert_eq!(call.provider(), "openai", "Provider should be openai");
}

/// Test multiple clients - Go: TestClientRegistryMultipleClients
#[test]
fn test_multiple_clients() {
    let mut registry = ClientRegistry::new();

    // Add multiple clients
    for i in 1..=3 {
        let mut options = HashMap::new();
        let model = if i == 2 {
            "gpt-4o-mini"
        } else {
            "gpt-3.5-turbo"
        };
        options.insert("model".to_string(), serde_json::json!(model));
        registry.add_llm_client(&format!("Client{}", i), "openai", options);
    }

    // Test switching between clients
    for i in 1..=3 {
        registry.set_primary_client(&format!("Client{}", i));

        let result = B
            .TestOpenAIGPT4oMini
            .with_client_registry(&registry)
            .call(&format!("test with Client{}", i));

        assert!(
            result.is_ok(),
            "Expected successful call from Client{}, got {:?}",
            i,
            result
        );
        let output = result.unwrap();
        // Go: assert.NotEmpty(t, result)
        assert!(
            !output.is_empty(),
            "Expected non-empty result from Client{}",
            i
        );
    }
}
