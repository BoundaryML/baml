use std::collections::{HashMap, HashSet};

use base64::write;
use colored::*;
pub mod llm_provider;
pub mod orchestrator;
pub mod primitive;

pub mod retry_policy;
mod strategy;
pub mod traits;

use anyhow::Result;

use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage, RenderedPrompt};
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::error::Error;

use reqwest::StatusCode;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[derive(Clone, Copy, PartialEq)]
pub enum ResolveMediaUrls {
    // there are 5 input formats:
    // - file
    // - url_with_mime
    // - url_no_mime
    // - b64_with_mime
    // - b64_no_mime

    // there are 5 possible output formats:
    // - url_with_mime: vertex
    // - url_no_mime: openai
    // - b64_with_mime: everyone (aws, anthropic, google, openai, vertex)
    // - b64_no_mime: no one

    // aws: supports b64 w mime
    // anthropic: supports b64 w mime
    // google: supports b64 w mime
    // openai: supports URLs w/o mime (b64 data URLs also work here)
    // vertex: supports URLs w/ mime, b64 w/ mime
    Always,
    EnsureMime,
    Never,
}

#[derive(Clone)]
pub struct ModelFeatures {
    pub completion: bool,
    pub chat: bool,
    pub anthropic_system_constraints: bool,
    pub resolve_media_urls: ResolveMediaUrls,
    pub allowed_metadata: AllowedMetadata,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AllowedMetadata {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "none")]
    None,
    Only(HashSet<String>),
}

impl AllowedMetadata {
    pub fn is_allowed(&self, key: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Only(allowed) => allowed.contains(&key.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct RetryLLMResponse {
    pub client: Option<String>,
    pub passed: Option<Box<LLMResponse>>,
    pub failed: Vec<LLMResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub enum LLMResponse {
    /// BAML was able to successfully make the HTTP request and got a 2xx
    /// response from the model provider
    Success(LLMCompleteResponse),
    /// Usually: BAML was able to successfully make the HTTP request, but the
    /// model provider returned a non-2xx response
    LLMFailure(LLMErrorResponse),
    /// BAML failed to make an HTTP request to a model, because the user's args
    /// failed to pass validation
    UserFailure(String),
    /// BAML failed to make an HTTP request to a model, because of some internal
    /// error after the user's args passed validation
    InternalFailure(String),
}

impl Error for LLMResponse {}

impl crate::tracing::Visualize for LLMResponse {
    fn visualize(&self, max_chunk_size: usize) -> String {
        match self {
            Self::Success(response) => response.visualize(max_chunk_size),
            Self::LLMFailure(failure) => failure.visualize(max_chunk_size),
            Self::UserFailure(message) => {
                format!(
                    "{}",
                    format!("Failed before LLM call (user error): {message}").red()
                )
            }
            Self::InternalFailure(message) => {
                format!("{}", format!("Failed before LLM call: {message}").red())
            }
        }
    }
}

impl std::fmt::Display for LLMResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(response) => write!(f, "{}", response),
            Self::LLMFailure(failure) => write!(f, "LLM call failed: {failure:?}"),
            Self::UserFailure(message) => {
                write!(f, "Failed before LLM call (user error): {message}")
            }
            Self::InternalFailure(message) => write!(f, "Failed before LLM call: {message}"),
        }
    }
}

impl LLMResponse {
    pub fn content(&self) -> Result<&str> {
        match self {
            Self::Success(response) => Ok(&response.content),
            Self::LLMFailure(failure) => Err(anyhow::anyhow!("LLM call failed: {failure:?}")),
            Self::UserFailure(message) => Err(anyhow::anyhow!(
                "Failed before LLM call (user error): {message}"
            )),
            Self::InternalFailure(message) => {
                Err(anyhow::anyhow!("Failed before LLM call: {message}"))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMErrorResponse {
    pub client: String,
    pub model: Option<String>,
    pub prompt: RenderedPrompt,
    pub request_options: HashMap<String, serde_json::Value>,
    #[cfg_attr(target_arch = "wasm32", serde(skip_serializing))]
    pub start_time: web_time::SystemTime,
    pub latency: web_time::Duration,

    // Short error message
    pub message: String,
    pub code: ErrorCode,
}

#[derive(Debug, Clone, Serialize)]
pub enum ErrorCode {
    InvalidAuthentication, // 401
    NotSupported,          // 403
    RateLimited,           // 429
    ServerError,           // 500
    ServiceUnavailable,    // 503

    // We failed to parse the response
    UnsupportedResponse(u16),

    // Any other error
    Other(u16),
}

impl ErrorCode {
    pub fn to_string(&self) -> String {
        match self {
            ErrorCode::InvalidAuthentication => "InvalidAuthentication (401)".into(),
            ErrorCode::NotSupported => "NotSupported (403)".into(),
            ErrorCode::RateLimited => "RateLimited (429)".into(),
            ErrorCode::ServerError => "ServerError (500)".into(),
            ErrorCode::ServiceUnavailable => "ServiceUnavailable (503)".into(),
            ErrorCode::UnsupportedResponse(code) => format!("BadResponse {}", code),
            ErrorCode::Other(code) => format!("Unspecified error code: {}", code),
        }
    }

    pub fn from_status(status: StatusCode) -> Self {
        match status.as_u16() {
            401 => ErrorCode::InvalidAuthentication,
            403 => ErrorCode::NotSupported,
            429 => ErrorCode::RateLimited,
            500 => ErrorCode::ServerError,
            503 => ErrorCode::ServiceUnavailable,
            code => ErrorCode::Other(code),
        }
    }

    pub fn from_u16(code: u16) -> Self {
        match code {
            401 => ErrorCode::InvalidAuthentication,
            403 => ErrorCode::NotSupported,
            429 => ErrorCode::RateLimited,
            500 => ErrorCode::ServerError,
            503 => ErrorCode::ServiceUnavailable,
            code => ErrorCode::Other(code),
        }
    }

    pub fn to_u16(&self) -> u16 {
        match self {
            ErrorCode::InvalidAuthentication => 401,
            ErrorCode::NotSupported => 403,
            ErrorCode::RateLimited => 429,
            ErrorCode::ServerError => 500,
            ErrorCode::ServiceUnavailable => 503,
            ErrorCode::UnsupportedResponse(code) => *code,
            ErrorCode::Other(code) => *code,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LLMCompleteResponse {
    pub client: String,
    pub model: String,
    pub prompt: RenderedPrompt,
    pub request_options: HashMap<String, serde_json::Value>,
    pub content: String,
    #[cfg_attr(target_arch = "wasm32", serde(skip_serializing))]
    pub start_time: web_time::SystemTime,
    pub latency: web_time::Duration,
    pub metadata: LLMCompleteResponseMetadata,
}

#[derive(Clone, Debug, Serialize)]
pub struct LLMCompleteResponseMetadata {
    pub baml_is_complete: bool,
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

impl std::fmt::Display for LLMCompleteResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}",
            format!(
                "Client: {} ({}) - {}ms. StopReason: {}",
                self.client,
                self.model,
                self.latency.as_millis(),
                self.metadata.finish_reason.as_deref().unwrap_or("unknown")
            )
            .yellow()
        )?;
        writeln!(f, "{}", "---PROMPT---".blue())?;
        writeln!(f, "{}", self.prompt.to_string().dimmed())?;
        writeln!(f, "{}", "---LLM REPLY---".blue())?;
        write!(f, "{}", self.content.dimmed())
    }
}

impl crate::tracing::Visualize for LLMCompleteResponse {
    fn visualize(&self, max_chunk_size: usize) -> String {
        let s = vec![
            format!(
                "{}",
                format!(
                    "Client: {} ({}) - {}ms. StopReason: {}",
                    self.client,
                    self.model,
                    self.latency.as_millis(),
                    self.metadata.finish_reason.as_deref().unwrap_or("unknown")
                )
                .yellow()
            ),
            format!("{}", "---PROMPT---".blue()),
            format!(
                "{}",
                crate::tracing::truncate_string(&self.prompt.to_string(), max_chunk_size).dimmed()
            ),
            format!("{}", "---LLM REPLY---".blue()),
            format!(
                "{}",
                crate::tracing::truncate_string(&self.content, max_chunk_size).dimmed()
            ),
        ];
        s.join("\n")
    }
}

impl crate::tracing::Visualize for LLMErrorResponse {
    fn visualize(&self, max_chunk_size: usize) -> String {
        let mut s = vec![
            format!(
                "{}",
                format!(
                    "Client: {} ({}) - {}ms",
                    self.client,
                    self.model.as_deref().unwrap_or("<unknown>"),
                    self.latency.as_millis(),
                )
                .yellow(),
            ),
            format!("{}", "---PROMPT---".blue()),
            format!(
                "{}",
                crate::tracing::truncate_string(&self.prompt.to_string(), max_chunk_size).dimmed()
            ),
            format!("{}", "---REQUEST OPTIONS---".blue()),
        ];
        for (k, v) in &self.request_options {
            s.push(format!(
                "{}: {}",
                k,
                crate::tracing::truncate_string(&v.to_string(), max_chunk_size)
            ));
        }
        s.push(format!(
            "{}",
            format!("---ERROR ({})---", self.code.to_string()).red()
        ));
        s.push(format!(
            "{}",
            crate::tracing::truncate_string(&self.message, max_chunk_size).red()
        ));
        s.join("\n")
    }
}

// For parsing args
fn resolve_properties_walker(
    client: &ClientWalker,
    ctx: &crate::RuntimeContext,
) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    use anyhow::Context;
    (&client.item.elem.options)
        .iter()
        .map(|(k, v)| {
            Ok((
                k.into(),
                ctx.resolve_expression::<serde_json::Value>(v)
                    .context(format!(
                        "client {} could not resolve options.{}",
                        client.name(),
                        k
                    ))?,
            ))
        })
        .collect::<Result<std::collections::HashMap<_, _>>>()
}
