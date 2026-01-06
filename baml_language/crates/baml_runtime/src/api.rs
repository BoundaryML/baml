//! Runtime API - Entry points for executing BAML functions.
//!
//! This module provides the main entry points for the BAML runtime:
//!
//! - `call_function` - Execute a function synchronously
//! - `stream_function` - Execute a function with streaming
//! - `run_test` - Execute a test with constraint evaluation
//! - `render_prompt` - Render a prompt without executing
//! - `build_request` - Build a provider-specific request
//! - `render_raw_curl` - Generate a curl command

use crate::context::{PerCallContext, SharedCallContext};
use crate::errors::RuntimeError;
use crate::llm_request::openai::{OpenAiClientConfig, OpenAiRequest};
use crate::orchestrator::{
    ClientConfig, FunctionResultStream, OrchestratorConfig, OrchestratorNode,
    OrchestrationScope, ProviderType, orchestrate_call, orchestrate_stream,
};
use crate::prepared_function::PreparedFunction;
use crate::prompt::RenderedPrompt;
use crate::render_options::RenderOptions;
use crate::types::{BamlValue, FunctionResult, TestResult};

/// Execute a function and wait for the complete result.
///
/// This is the primary entry point for non-streaming function execution.
pub fn call_function(
    prepared: &PreparedFunction,
    _shared_ctx: &SharedCallContext,
    per_call_ctx: &PerCallContext,
) -> Result<FunctionResult, RuntimeError> {
    // Build orchestrator config from prepared function
    let config = build_orchestrator_config(&prepared.client_spec)?;

    // Render the prompt
    let prompt = render_prompt(prepared)?;

    // Execute through orchestrator
    let result = orchestrate_call(
        &prompt,
        &config,
        &per_call_ctx.env_vars,
        &prepared.output_type,
        || per_call_ctx.is_cancelled(),
    )?;

    // Convert to FunctionResult
    Ok(FunctionResult {
        value: result.response.map(|r| r.value).unwrap_or(BamlValue::Null),
        attempts: result.attempts.iter().map(|a| crate::types::OrchestrationAttemptSummary {
            client_name: a.node.client.name.clone(),
            success: a.error.is_none(),
            error: a.error.as_ref().map(|e| e.to_string()),
            duration: a.duration,
        }).collect(),
        duration: result.total_duration,
    })
}

/// Execute a function with streaming, returning a stream handle.
///
/// The caller is responsible for driving the stream to completion.
pub fn stream_function(
    prepared: &PreparedFunction,
    _shared_ctx: &SharedCallContext,
    per_call_ctx: &PerCallContext,
) -> Result<FunctionResultStream, RuntimeError> {
    // Build orchestrator config from prepared function
    let config = build_orchestrator_config(&prepared.client_spec)?;

    // Render the prompt
    let prompt = render_prompt(prepared)?;

    // Create stream through orchestrator
    orchestrate_stream(
        &prompt,
        config,
        &per_call_ctx.env_vars,
        prepared.output_type.clone(),
        || per_call_ctx.is_cancelled(),
    )
}

/// Execute a test, evaluating @assert/@check constraints.
pub fn run_test(
    prepared: &PreparedFunction,
    shared_ctx: &SharedCallContext,
    per_call_ctx: &PerCallContext,
) -> Result<TestResult, RuntimeError> {
    // Execute the function
    let function_result = call_function(prepared, shared_ctx, per_call_ctx)?;

    // TODO: Evaluate constraints from the function definition
    // For now, return empty constraint results
    Ok(TestResult {
        function_result,
        constraint_results: vec![],
    })
}

/// Render a prompt without executing.
///
/// This is useful for debugging and previewing prompts.
pub fn render_prompt(prepared: &PreparedFunction) -> Result<RenderedPrompt, RuntimeError> {
    // TODO: Integrate with jinja runtime for real template rendering
    // For now, create a simple prompt from the template

    // Simple variable substitution (placeholder for real jinja rendering)
    let mut rendered_template = prepared.prompt_template.template.clone();

    for (key, value) in &prepared.args {
        let pattern = format!("{{{{ {} }}}}", key);
        let replacement = match value {
            BamlValue::String(s) => s.clone(),
            BamlValue::Int(i) => i.to_string(),
            BamlValue::Float(f) => f.to_string(),
            BamlValue::Bool(b) => b.to_string(),
            _ => serde_json::to_string(value).unwrap_or_default(),
        };
        rendered_template = rendered_template.replace(&pattern, &replacement);
    }

    Ok(RenderedPrompt::simple(rendered_template))
}

/// Build a provider-specific request without executing.
///
/// Returns the request that would be sent to the LLM provider.
pub fn build_request(
    prepared: &PreparedFunction,
    per_call_ctx: &PerCallContext,
    stream: bool,
) -> Result<OpenAiRequest, RuntimeError> {
    let prompt = render_prompt(prepared)?;

    let client_config = OpenAiClientConfig {
        api_key: per_call_ctx.env_vars.get("OPENAI_API_KEY").cloned().unwrap_or_default(),
        model: "gpt-4".to_string(), // TODO: Get from client spec
        ..Default::default()
    };

    OpenAiRequest::from_rendered(&prompt, &client_config, stream)
        .map_err(RuntimeError::from)
}

/// Generate a curl command for the request.
///
/// This is useful for debugging and sharing requests.
pub fn render_raw_curl(
    prepared: &PreparedFunction,
    per_call_ctx: &PerCallContext,
    options: &RenderOptions,
) -> Result<String, RuntimeError> {
    let request = build_request(prepared, per_call_ctx, false)?;
    Ok(request.to_curl(options))
}

/// Build orchestrator config from client specification.
fn build_orchestrator_config(
    client_spec: &ir_stub::ClientSpec,
) -> Result<OrchestratorConfig, RuntimeError> {
    // For now, create a simple single-node config
    // TODO: Parse retry/fallback from client_spec

    let provider = if client_spec.client_name.to_lowercase().contains("anthropic") {
        ProviderType::Anthropic
    } else {
        ProviderType::OpenAi
    };

    let node = OrchestratorNode {
        client: ClientConfig {
            name: client_spec.client_name.clone(),
            provider,
            options: serde_json::json!({}),
        },
        scope: OrchestrationScope::Direct,
        delay: None,
    };

    Ok(OrchestratorConfig::single(node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BamlMap;
    use ir_stub::{ClientSpec, PromptTemplate, TypeRef};
    use std::collections::HashMap;

    fn create_test_prepared() -> PreparedFunction {
        let mut args = BamlMap::new();
        args.insert("name".to_string(), BamlValue::from("Alice"));

        PreparedFunction::new_stub(
            "Greet",
            args,
            TypeRef::string(),
            ClientSpec::new("openai/gpt-4"),
            PromptTemplate::new("Hello, {{ name }}!"),
        )
    }

    #[test]
    fn test_render_prompt() {
        let prepared = create_test_prepared();
        let result = render_prompt(&prepared);

        assert!(result.is_ok());
        let prompt = result.unwrap();
        assert_eq!(prompt.messages.len(), 1);
        assert_eq!(prompt.messages[0].text_content(), "Hello, Alice!");
    }

    #[test]
    fn test_build_request() {
        let prepared = create_test_prepared();
        let ctx = PerCallContext::new();

        let result = build_request(&prepared, &ctx, false);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert!(!request.stream);
        assert!(request.url.contains("chat/completions"));
    }

    #[test]
    fn test_render_raw_curl() {
        let prepared = create_test_prepared();
        let ctx = PerCallContext::new();
        let options = RenderOptions::default();

        let result = render_raw_curl(&prepared, &ctx, &options);
        assert!(result.is_ok());

        let curl = result.unwrap();
        assert!(curl.contains("curl"));
        assert!(curl.contains("-X POST"));
        assert!(curl.contains("[REDACTED]")); // API key should be masked
    }

    #[test]
    fn test_render_raw_curl_with_secrets() {
        let prepared = create_test_prepared();
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-test-key".to_string());
        let ctx = PerCallContext::new().with_env_vars(env);
        let options = RenderOptions::for_execution();

        let result = render_raw_curl(&prepared, &ctx, &options);
        assert!(result.is_ok());

        let curl = result.unwrap();
        assert!(curl.contains("sk-test-key")); // API key should be visible
    }
}
