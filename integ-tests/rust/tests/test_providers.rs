//! Provider tests - ported from test_providers_test.go
//!
//! Tests for various LLM providers including:
//! - OpenAI
//! - Anthropic
//! - Google/Gemini
//! - AWS Bedrock
//! - Azure

use baml::ClientRegistry;
use rust::baml_client::sync_client::B;
use std::collections::HashMap;

/// Test OpenAI shorthand - Go: TestOpenAIProvider/OpenAIShorthand
#[test]
fn test_openai_shorthand() {
    let result = B.TestOpenAIShorthand.call("Mt Rainier is tall");
    assert!(
        result.is_ok(),
        "Expected successful OpenAI shorthand call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from OpenAI shorthand")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from OpenAI shorthand"
    );
}

/// Test OpenAI GPT-4o-mini - Go: TestOpenAIProvider/OpenAIGPT4oMini
#[test]
fn test_openai_gpt4o_mini() {
    let result = B.TestOpenAIGPT4oMini.call("test input");
    assert!(
        result.is_ok(),
        "Expected successful GPT-4o-mini call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from GPT-4o-mini")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from GPT-4o-mini"
    );
}

/// Test OpenAI with finish reason error - Go: TestOpenAIProvider/OpenAIWithFinishReasonError
#[test]
fn test_openai_finish_reason_error() {
    let result = B.TestOpenAIWithFinishReasonError.call("test");
    // Go: assert.Error(t, err, "Expected finish reason error")
    assert!(result.is_err(), "Expected finish reason error");
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "Finish reason error:")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("Finish reason"),
        "Expected 'Finish reason' in error, got: {}",
        error_str
    );
}

/// Test OpenAI provider basic
#[test]
fn test_openai_provider() {
    let result = B.TestOpenAI.call("Hello from Rust");
    assert!(
        result.is_ok(),
        "Expected successful OpenAI call, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty response");
}

/// Test Anthropic shorthand - Go: TestAnthropicProvider/AnthropicShorthand
#[test]
fn test_anthropic_shorthand() {
    let result = B.TestAnthropicShorthand.call("Mt Rainier is tall");
    assert!(
        result.is_ok(),
        "Expected successful Anthropic shorthand call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from Anthropic shorthand")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from Anthropic shorthand"
    );
}

/// Test Claude prompt - Go: TestAnthropicProvider/PromptTestClaude
#[test]
fn test_claude_prompt() {
    let result = B.PromptTestClaude.call("Mt Rainier is tall");
    assert!(
        result.is_ok(),
        "Expected successful Claude call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from Claude")
    assert!(!output.is_empty(), "Expected non-empty result from Claude");
}

/// Test Anthropic provider basic
#[test]
fn test_anthropic_provider() {
    let result = B.TestAnthropic.call("Hello from Rust");
    assert!(
        result.is_ok(),
        "Expected successful Anthropic call, got {:?}",
        result
    );
}

/// Test Google/Gemini provider - Go: TestGoogleProvider/TestGemini
#[test]
fn test_gemini_provider() {
    let result = B.TestGemini.call("Dr. Pepper");
    assert!(
        result.is_ok(),
        "Expected successful Gemini call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from Gemini")
    assert!(!output.is_empty(), "Expected non-empty result from Gemini");
}

/// Test Gemini with system instructions - Go: TestGoogleProvider/TestGeminiSystem
#[test]
fn test_gemini_system() {
    let result = B.TestGeminiSystem.call("Dr. Pepper");
    assert!(
        result.is_ok(),
        "Expected successful Gemini system call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from Gemini with system prompt")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from Gemini with system prompt"
    );
}

/// Test AWS Bedrock provider - Go: TestAWSBedrockProvider/TestAws
#[test]
fn test_aws_bedrock_provider() {
    let result = B.TestAws.call("Mt Rainier is tall");
    assert!(
        result.is_ok(),
        "Expected successful AWS call, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from AWS")
    assert!(!output.is_empty(), "Expected non-empty result from AWS");
}

/// Test AWS invalid region - Go: TestAWSBedrockProvider/TestAwsInvalidRegion
#[test]
fn test_aws_invalid_region() {
    let result = B.TestAwsInvalidRegion.call("lightning in a rock");
    // Go: assert.Error(t, err, "Expected error for invalid AWS region")
    assert!(result.is_err(), "Expected error for invalid AWS region");
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "DispatchFailure")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("DispatchFailure"),
        "Expected 'DispatchFailure' in error, got: {}",
        error_str
    );
}

/// Test Vertex AI provider - Go: TestGoogleProvider/TestVertex
#[test]
fn test_vertex_provider() {
    let result = B.TestVertex.call("donkey kong");
    assert!(
        result.is_ok(),
        "Expected successful Vertex call, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: assert.Contains(t, strings.ToLower(result), "donkey kong")
    assert!(
        output.contains("donkey kong"),
        "Expected result to contain 'donkey kong', got: {}",
        output
    );
}

/// Test Azure provider
#[test]
fn test_azure_provider() {
    let result = B.TestAzure.call("Hello from Rust");
    // May succeed or fail depending on Azure config - just verify no panic
    match result {
        Ok(output) => {
            assert!(
                !output.is_empty(),
                "Expected non-empty result when Azure succeeds"
            );
        }
        Err(_e) => {
            // Azure may not be configured, that's okay
        }
    }
}

/// Test OpenAI Responses API - Go: TestProviderSpecificFeatures/OpenAIResponsesAPI
#[test]
fn test_openai_responses() {
    let result = B.TestOpenAIResponses.call("mountains");
    // Go: May skip if not available
    match result {
        Ok(output) => {
            // Go: assert.NotEmpty(t, result, "Expected non-empty result from OpenAI Responses")
            assert!(
                !output.is_empty(),
                "Expected non-empty result from OpenAI Responses"
            );
        }
        Err(_e) => {
            // OpenAI Responses API may not be available
            eprintln!("OpenAI Responses API not available, skipping");
        }
    }
}

/// Test provider fallbacks - Go: TestProviderFallbacks/TestFallbackClient
#[test]
fn test_provider_fallbacks() {
    let result = B.TestFallbackClient.call();
    assert!(
        result.is_ok(),
        "Expected successful fallback, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result from fallback client")
    assert!(
        !output.is_empty(),
        "Expected non-empty result from fallback client"
    );
}

/// Test single fallback client - Go: TestProviderFallbacks/TestSingleFallbackClient
#[test]
fn test_single_fallback_client() {
    let result = B.TestSingleFallbackClient.call();
    // Go: assert.Error(t, err, "Expected connection error from single fallback client")
    assert!(
        result.is_err(),
        "Expected connection error from single fallback client"
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

/// Test provider with dynamic clients - Go: TestProviderWithDynamicClients/DynamicOpenAIClient
#[test]
fn test_dynamic_openai_client() {
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
    // Go: assert.Contains(t, strings.ToLower(result), "london")
    assert!(
        output.contains("london"),
        "Expected result to contain 'london', got: {}",
        output
    );
}

/// Test provider client response type mismatch - Go: TestProviderClientResponseTypes
#[test]
fn test_client_response_type_mismatch() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert(
        "client_response_type".to_string(),
        serde_json::json!("anthropic"),
    );
    options.insert("model".to_string(), serde_json::json!("gpt-4o"));
    registry.add_llm_client("temp_client", "openai", options);
    registry.set_primary_client("temp_client");

    let result = B.TestOpenAI.with_client_registry(&registry).call("test");
    // Go: assert.Error(t, err, "Expected error due to client response type mismatch")
    assert!(
        result.is_err(),
        "Expected error due to client response type mismatch"
    );
}

/// Test provider specific error - invalid key - Go: TestProviderSpecificErrors/OpenAIInvalidKey
#[test]
fn test_openai_invalid_key() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("gpt-4o-mini"));
    options.insert("api_key".to_string(), serde_json::json!("INVALID_KEY"));
    registry.add_llm_client("InvalidClient", "openai", options);
    registry.set_primary_client("InvalidClient");

    let result = B
        .TestOpenAIGPT4oMini
        .with_client_registry(&registry)
        .call("test");
    // Go: assert.Error(t, err, "Expected authentication error")
    assert!(result.is_err(), "Expected authentication error");
    let error = result.unwrap_err();
    // Go: assert.Contains(t, err.Error(), "401")
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("401"),
        "Expected '401' in error, got: {}",
        error_str
    );
}

/// Test provider specific error - invalid model - Go: TestProviderSpecificErrors/OpenAIInvalidModel
#[test]
fn test_openai_invalid_model() {
    let mut registry = ClientRegistry::new();
    let mut options = HashMap::new();
    options.insert("model".to_string(), serde_json::json!("random-model"));
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
