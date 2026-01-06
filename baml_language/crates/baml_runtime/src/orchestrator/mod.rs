//! Orchestrator - manages retry/fallback logic for LLM calls.
//!
//! The orchestrator has two main entry points:
//! - `orchestrate_call` - Non-streaming, waits for complete response
//! - `orchestrate_stream` - Streaming, returns a stream for caller to drive

mod common;

pub use common::*;

use std::time::Duration;

use crate::errors::RuntimeError;
use crate::llm_response::LLMResponse;
use crate::prompt::RenderedPrompt;
use crate::types::BamlValue;

/// Configuration for the orchestrator.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Nodes to try (with retry/fallback strategies baked in).
    pub nodes: Vec<OrchestratorNode>,
}

impl OrchestratorConfig {
    /// Create a config with a single node.
    pub fn single(node: OrchestratorNode) -> Self {
        Self { nodes: vec![node] }
    }

    /// Create a config with multiple nodes for fallback.
    pub fn with_fallback(nodes: Vec<OrchestratorNode>) -> Self {
        Self { nodes }
    }
}

/// A single orchestration node (client + retry config).
#[derive(Debug, Clone)]
pub struct OrchestratorNode {
    /// Client configuration for this node.
    pub client: ClientConfig,
    /// Scope describing how we got here (retry, fallback, etc.).
    pub scope: OrchestrationScope,
    /// Delay before this attempt (for retries).
    pub delay: Option<Duration>,
}

/// Client configuration for an orchestration node.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Name of the client.
    pub name: String,
    /// Provider type.
    pub provider: ProviderType,
    /// Provider-specific options.
    pub options: serde_json::Value,
}

/// Supported LLM providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderType {
    OpenAi,
    Anthropic,
    // Add more as needed
}

/// Scope describing the orchestration path.
#[derive(Debug, Clone)]
pub enum OrchestrationScope {
    /// Initial attempt.
    Direct,
    /// Retry of a previous attempt.
    Retry { attempt: usize },
    /// Fallback to a different client.
    Fallback { from_client: String },
}

/// Result of an orchestration run.
#[derive(Debug, Clone)]
pub struct OrchestrationResult {
    /// The successful response (if any).
    pub response: Option<ParsedResponse>,
    /// All attempts made.
    pub attempts: Vec<OrchestrationAttempt>,
    /// Total time spent (including sleeps).
    pub total_duration: Duration,
}

/// A successfully parsed response.
#[derive(Debug, Clone)]
pub struct ParsedResponse {
    /// The parsed BAML value.
    pub value: BamlValue,
    /// Raw content from the LLM.
    pub raw_content: String,
    /// Model that generated this response.
    pub model: String,
    /// Response metadata.
    pub metadata: crate::llm_response::LLMResponseMetadata,
}

/// Record of a single orchestration attempt.
#[derive(Debug, Clone)]
pub struct OrchestrationAttempt {
    /// The node that was used.
    pub node: OrchestratorNode,
    /// Rendered prompt (if rendering succeeded).
    pub rendered_prompt: Option<RenderedPrompt>,
    /// Raw LLM response (if request succeeded).
    pub llm_response: Option<LLMResponse>,
    /// Parsed output (if parsing was attempted).
    pub parsed_output: Option<Result<BamlValue, String>>,
    /// Error that occurred (if any).
    pub error: Option<RuntimeError>,
    /// Duration of this attempt.
    pub duration: Duration,
}

impl OrchestrationAttempt {
    /// Create a new attempt record.
    pub fn new(node: OrchestratorNode) -> Self {
        Self {
            node,
            rendered_prompt: None,
            llm_response: None,
            parsed_output: None,
            error: None,
            duration: Duration::ZERO,
        }
    }

    /// Create a failed attempt record.
    pub fn failed(node: OrchestratorNode, error: RuntimeError, duration: Duration) -> Self {
        Self {
            node,
            rendered_prompt: None,
            llm_response: None,
            parsed_output: None,
            error: Some(error),
            duration,
        }
    }
}

// ============================================================================
// Streaming Types
// ============================================================================

/// Streaming result that handles SSE consumption and parsing.
pub struct FunctionResultStream {
    /// Current orchestration node being used.
    pub node: OrchestratorNode,
    /// Current node index.
    pub node_index: usize,
    /// Remaining nodes to try on failure.
    pub remaining_nodes: Vec<OrchestratorNode>,
    /// Accumulated content from the stream.
    pub accumulated_content: String,
    /// Attempts made so far.
    pub attempts: Vec<OrchestrationAttempt>,
    /// Start time for duration tracking.
    pub start_time: std::time::Instant,
    /// Output type for parsing.
    pub output_type: ir_stub::TypeRef,
}

impl FunctionResultStream {
    /// Get the accumulated content so far.
    pub fn content(&self) -> &str {
        &self.accumulated_content
    }

    /// Append content from a streaming chunk.
    pub fn append_content(&mut self, chunk: &str) {
        self.accumulated_content.push_str(chunk);
    }

    /// Parse the current accumulated content as a partial result.
    pub fn parse_partial(&self) -> Result<crate::types::PartialResult, crate::errors::ParseOutputError> {
        let value = crate::parsing::parse_output_partial(&self.accumulated_content, &self.output_type)?;
        Ok(crate::types::PartialResult {
            value,
            raw_content: self.accumulated_content.clone(),
        })
    }

    /// Finalize the stream and return the parsed result.
    pub fn finalize(self) -> Result<OrchestrationResult, RuntimeError> {
        let value = crate::parsing::parse_output(&self.accumulated_content, &self.output_type)?;

        Ok(OrchestrationResult {
            response: Some(ParsedResponse {
                value,
                raw_content: self.accumulated_content,
                model: "unknown".to_string(), // TODO: Track model from stream
                metadata: crate::llm_response::LLMResponseMetadata::default(),
            }),
            attempts: self.attempts,
            total_duration: self.start_time.elapsed(),
        })
    }
}

// ============================================================================
// Orchestration Functions
// ============================================================================

/// Non-streaming orchestrator - iterates through nodes until success or exhaustion.
///
/// This is a simplified implementation that doesn't actually execute HTTP requests.
/// It's designed to be completed when the HTTP execution layer is wired up.
pub fn orchestrate_call(
    prompt: &RenderedPrompt,
    config: &OrchestratorConfig,
    env_vars: &std::collections::HashMap<String, String>,
    _output_type: &ir_stub::TypeRef,
    is_cancelled: impl Fn() -> bool,
) -> Result<OrchestrationResult, RuntimeError> {
    use crate::errors::RetryableError;
    use std::time::Instant;

    let mut attempts = Vec::new();
    let _start = Instant::now();

    for (node_index, node) in config.nodes.iter().enumerate() {
        // Check cancellation at loop boundary only
        if is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }

        // Apply delay if specified (for retries)
        if let Some(delay) = node.delay {
            std::thread::sleep(delay);
        }

        let attempt_start = Instant::now();
        let mut attempt = OrchestrationAttempt::new(node.clone());

        // Prepare the request
        let _prepared = match prepare_node_request(prompt, node, env_vars, false) {
            Ok(p) => {
                attempt.rendered_prompt = Some(p.rendered_prompt.clone());
                p
            }
            Err(e) => {
                attempt.error = Some(e);
                attempt.duration = attempt_start.elapsed();
                attempts.push(attempt);
                continue;
            }
        };

        // NOTE: HTTP execution would happen here
        // For now, we return a placeholder error indicating execution is not implemented
        // This allows the types and orchestration logic to be tested without real HTTP

        let error = RuntimeError::Http(crate::errors::HttpError {
            message: "HTTP execution not implemented - use WASM bindings for actual calls".to_string(),
            status_code: None,
        });

        if error.is_retryable() && node_index + 1 < config.nodes.len() {
            attempt.error = Some(error);
            attempt.duration = attempt_start.elapsed();
            attempts.push(attempt);
            continue;
        } else {
            attempt.error = Some(error.clone());
            attempt.duration = attempt_start.elapsed();
            attempts.push(attempt);

            return Err(RuntimeError::OrchestrationExhausted {
                attempts: attempts.len(),
                errors: attempts
                    .iter()
                    .filter_map(|a| a.error.clone())
                    .collect(),
            });
        }
    }

    Err(RuntimeError::OrchestrationExhausted {
        attempts: attempts.len(),
        errors: attempts
            .iter()
            .filter_map(|a| a.error.clone())
            .collect(),
    })
}

/// Streaming orchestrator - prepares a stream for the caller to drive.
///
/// Returns a FunctionResultStream that the caller can use to:
/// 1. Send the HTTP request
/// 2. Process SSE events
/// 3. Parse partial and final results
pub fn orchestrate_stream(
    prompt: &RenderedPrompt,
    config: OrchestratorConfig,
    env_vars: &std::collections::HashMap<String, String>,
    output_type: ir_stub::TypeRef,
    is_cancelled: impl Fn() -> bool,
) -> Result<FunctionResultStream, RuntimeError> {
    use std::time::Instant;

    let mut attempts = Vec::new();
    let start = Instant::now();

    for (node_index, node) in config.nodes.iter().enumerate() {
        // Check cancellation
        if is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }

        // Apply delay if specified
        if let Some(delay) = node.delay {
            std::thread::sleep(delay);
        }

        let attempt_start = Instant::now();

        // Prepare the request (with stream=true)
        match prepare_node_request(prompt, node, env_vars, true) {
            Ok(_prepared) => {
                // Success - return stream for caller to drive
                return Ok(FunctionResultStream {
                    node: node.clone(),
                    node_index,
                    remaining_nodes: config.nodes[node_index + 1..].to_vec(),
                    accumulated_content: String::new(),
                    attempts,
                    start_time: start,
                    output_type,
                });
            }
            Err(e) => {
                attempts.push(OrchestrationAttempt::failed(
                    node.clone(),
                    e,
                    attempt_start.elapsed(),
                ));
                continue;
            }
        }
    }

    Err(RuntimeError::OrchestrationExhausted {
        attempts: attempts.len(),
        errors: attempts
            .iter()
            .filter_map(|a| a.error.clone())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_config_single() {
        let node = OrchestratorNode {
            client: ClientConfig {
                name: "openai".to_string(),
                provider: ProviderType::OpenAi,
                options: serde_json::json!({}),
            },
            scope: OrchestrationScope::Direct,
            delay: None,
        };

        let config = OrchestratorConfig::single(node);
        assert_eq!(config.nodes.len(), 1);
    }

    #[test]
    fn test_orchestrator_attempt() {
        let node = OrchestratorNode {
            client: ClientConfig {
                name: "openai".to_string(),
                provider: ProviderType::OpenAi,
                options: serde_json::json!({}),
            },
            scope: OrchestrationScope::Direct,
            delay: None,
        };

        let attempt = OrchestrationAttempt::new(node);
        assert!(attempt.error.is_none());
        assert!(attempt.llm_response.is_none());
    }

    #[test]
    fn test_orchestrate_stream_returns_stream() {
        use crate::prompt::RenderedPrompt;

        let prompt = RenderedPrompt::simple("Hello");
        let config = OrchestratorConfig::single(OrchestratorNode {
            client: ClientConfig {
                name: "openai".to_string(),
                provider: ProviderType::OpenAi,
                options: serde_json::json!({
                    "api_key": "sk-test",
                    "model": "gpt-4"
                }),
            },
            scope: OrchestrationScope::Direct,
            delay: None,
        });

        let result = orchestrate_stream(
            &prompt,
            config,
            &std::collections::HashMap::new(),
            ir_stub::TypeRef::string(),
            || false,
        );

        assert!(result.is_ok());
        let stream = result.unwrap();
        assert_eq!(stream.node_index, 0);
        assert!(stream.accumulated_content.is_empty());
    }

    #[test]
    fn test_orchestrate_stream_cancellation() {
        use crate::prompt::RenderedPrompt;

        let prompt = RenderedPrompt::simple("Hello");
        let config = OrchestratorConfig::single(OrchestratorNode {
            client: ClientConfig {
                name: "openai".to_string(),
                provider: ProviderType::OpenAi,
                options: serde_json::json!({ "api_key": "sk-test" }),
            },
            scope: OrchestrationScope::Direct,
            delay: None,
        });

        let result = orchestrate_stream(
            &prompt,
            config,
            &std::collections::HashMap::new(),
            ir_stub::TypeRef::string(),
            || true, // Already cancelled
        );

        assert!(matches!(result, Err(RuntimeError::Cancelled)));
    }

    #[test]
    fn test_function_result_stream_accumulate() {
        let mut stream = FunctionResultStream {
            node: OrchestratorNode {
                client: ClientConfig {
                    name: "test".to_string(),
                    provider: ProviderType::OpenAi,
                    options: serde_json::json!({}),
                },
                scope: OrchestrationScope::Direct,
                delay: None,
            },
            node_index: 0,
            remaining_nodes: vec![],
            accumulated_content: String::new(),
            attempts: vec![],
            start_time: std::time::Instant::now(),
            output_type: ir_stub::TypeRef::string(),
        };

        stream.append_content("Hello");
        stream.append_content(" world");
        assert_eq!(stream.content(), "Hello world");
    }

    #[test]
    fn test_function_result_stream_finalize() {
        let stream = FunctionResultStream {
            node: OrchestratorNode {
                client: ClientConfig {
                    name: "test".to_string(),
                    provider: ProviderType::OpenAi,
                    options: serde_json::json!({}),
                },
                scope: OrchestrationScope::Direct,
                delay: None,
            },
            node_index: 0,
            remaining_nodes: vec![],
            accumulated_content: r#"{"name": "Alice"}"#.to_string(),
            attempts: vec![],
            start_time: std::time::Instant::now(),
            output_type: ir_stub::TypeRef::new("object"),
        };

        let result = stream.finalize();
        assert!(result.is_ok());

        let orch_result = result.unwrap();
        assert!(orch_result.response.is_some());

        let response = orch_result.response.unwrap();
        if let crate::types::BamlValue::Map(map) = response.value {
            assert_eq!(map.get("name").and_then(|v| v.as_str()), Some("Alice"));
        } else {
            panic!("Expected Map value");
        }
    }
}
