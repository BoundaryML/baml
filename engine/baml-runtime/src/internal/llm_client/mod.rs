use colored::*;
// mod anthropic;
pub mod llm_provider;
mod openai;
pub mod retry_policy;
mod state;
pub mod traits;

use anyhow::Result;
use internal_baml_jinja::RenderedPrompt;

#[cfg(feature = "no_wasm")]
use reqwest::StatusCode;

#[cfg(not(feature = "no_wasm"))]
use wasm_bindgen::JsValue;

#[derive(Clone, Copy)]
pub struct ModelFeatures {
    pub completion: bool,
    pub chat: bool,
}

#[derive(Debug)]
pub struct RetryLLMResponse {
    pub client: Option<String>,
    pub passed: Option<Box<LLMResponse>>,
    pub failed: Vec<LLMResponse>,
}

#[derive(Debug)]
pub enum LLMResponse {
    Success(LLMCompleteResponse),
    LLMFailure(LLMErrorResponse),
    Retry(RetryLLMResponse),
    // #[cfg(not(feature = "no_wasm"))]
    // OtherFailures(JsValue),
    // #[cfg(feature = "no_wasm")]
    OtherFailures(String),
}

impl std::fmt::Display for LLMResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(response) => write!(f, "{}", response),
            Self::Retry(retry) => match retry.passed.as_ref() {
                Some(passed) => write!(f, "{}", passed),
                None => match retry.failed.last() {
                    Some(failed) => write!(f, "{}", failed),
                    None => write!(f, "retry policy specified, but 0 requests were issued"),
                },
            },
            Self::LLMFailure(failure) => write!(f, "LLM call failed: {failure:?}"),
            Self::OtherFailures(e) => write!(f, "LLM call failed for unknown reason: {e:?}"),
        }
    }
}

impl LLMResponse {
    pub fn content(&self) -> Result<&str> {
        match self {
            Self::Success(response) => Ok(&response.content),
            Self::Retry(retry) => match retry.passed.as_ref() {
                Some(passed) => passed.content(),
                None => match retry.failed.last() {
                    Some(failed) => failed.content(),
                    None => Err(anyhow::anyhow!(
                        "retry policy specified, but 0 requests were issued"
                    )),
                },
            },
            Self::LLMFailure(failure) => Err(anyhow::anyhow!("LLM call failed: {failure:?}")),
            Self::OtherFailures(e) => {
                Err(anyhow::anyhow!("LLM call failed for unknown reason: {e:?}"))
            }
        }
    }
}

#[derive(Debug)]
pub struct LLMErrorResponse {
    pub client: String,
    pub model: Option<String>,
    pub prompt: RenderedPrompt,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,

    // Short error message
    pub message: String,
    pub code: ErrorCode,
}

#[derive(Debug)]
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

    #[cfg(feature = "no_wasm")]
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
}

#[derive(Debug)]
pub struct LLMCompleteResponse {
    pub client: String,
    pub model: String,
    pub prompt: RenderedPrompt,
    pub content: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub metadata: serde_json::Value,
}

impl std::fmt::Display for LLMCompleteResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}",
            format!(
                "Client: {} ({}) - {}ms",
                self.client, self.model, self.latency_ms
            )
            .yellow()
        )?;
        writeln!(f, "{}", "---PROMPT---".blue())?;
        writeln!(f, "{}", self.prompt.to_string().dimmed())?;
        writeln!(f, "{}", "---LLM REPLY---".blue())?;
        write!(f, "{}", self.content.dimmed())
    }
}

pub struct LLMStreamResponse {
    pub delta: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub metadata: serde_json::Value,
}
