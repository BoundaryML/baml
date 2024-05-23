use colored::*;
// mod anthropic;
mod common;
pub mod llm_provider;
pub mod orchestrator;
mod primitive;
pub mod retry_policy;
mod state;
mod strategy;
pub mod traits;

use anyhow::Result;

use internal_baml_jinja::RenderedPrompt;
use serde::Serialize;

use reqwest::StatusCode;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[derive(Clone, Copy)]
pub struct ModelFeatures {
    pub completion: bool,
    pub chat: bool,
    pub anthropic_system_constraints: bool,
}

#[derive(Debug)]
pub struct RetryLLMResponse {
    pub client: Option<String>,
    pub passed: Option<Box<LLMResponse>>,
    pub failed: Vec<LLMResponse>,
}

#[derive(Debug, Clone)]
pub enum LLMResponse {
    Success(LLMCompleteResponse),
    LLMFailure(LLMErrorResponse),
    OtherFailure(String),
}

impl std::fmt::Display for LLMResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(response) => write!(f, "{}", response),
            Self::LLMFailure(failure) => write!(f, "LLM call failed: {failure:?}"),
            Self::OtherFailure(message) => write!(f, "LLM call failed: {message}"),
        }
    }
}

impl LLMResponse {
    pub fn content(&self) -> Result<&str> {
        match self {
            Self::Success(response) => Ok(&response.content),
            Self::LLMFailure(failure) => Err(anyhow::anyhow!("LLM call failed: {failure:?}")),
            Self::OtherFailure(message) => Err(anyhow::anyhow!("LLM failed to call: {message}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LLMErrorResponse {
    pub client: String,
    pub model: Option<String>,
    pub prompt: RenderedPrompt,
    pub start_time: web_time::SystemTime,
    pub latency: web_time::Duration,

    // Short error message
    pub message: String,
    pub code: ErrorCode,
}

#[derive(Debug, Clone)]
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
            ErrorCode::Other(code) => format!("Unspecified {}", code),
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
}

#[derive(Clone, Debug)]
pub struct LLMCompleteResponse {
    pub client: String,
    pub model: String,
    pub prompt: RenderedPrompt,
    pub content: String,
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
                "Client: {} ({}) - {}ms",
                self.client,
                self.model,
                self.latency.as_millis()
            )
            .yellow()
        )?;
        writeln!(f, "{}", "---PROMPT---".blue())?;
        writeln!(f, "{}", self.prompt.to_string().dimmed())?;
        writeln!(f, "{}", "---LLM REPLY---".blue())?;
        write!(f, "{}", self.content.dimmed())
    }
}
