use std::collections::{HashMap, HashSet};

use colored::*;
pub mod llm_provider;
pub mod orchestrator;
pub mod primitive;

pub mod retry_policy;
mod strategy;
pub mod traits;

use anyhow::{Context, Result};

use baml_types::{BamlMap, BamlValueWithMeta, FieldType, JinjaExpression, ResponseCheck};
use internal_baml_core::ir::{repr::IntermediateRepr, ClientWalker};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::AllowedRoleMetadata;
pub use jsonish::ResponseBamlValue;
use jsonish::{
    deserializer::{
        deserialize_flags::{constraint_results, DeserializerConditions, Flag},
        semantic_streaming::validate_streaming_state,
    },
    BamlValueWithFlags,
};
use serde::{Deserialize, Serialize};
use std::error::Error;

use reqwest::StatusCode;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

/// Validate a parsed value, checking asserts and checks.
pub fn parsed_value_to_response(
    ir: &IntermediateRepr,
    baml_value: BamlValueWithFlags,
    field_type: &FieldType,
    allow_partials: bool,
) -> Result<ResponseBamlValue> {
    let meta_flags: BamlValueWithMeta<Vec<Flag>> = baml_value.clone().into();
    let baml_value_with_meta: BamlValueWithMeta<Vec<(String, JinjaExpression, bool)>> =
        baml_value.clone().into();

    let value_with_response_checks: BamlValueWithMeta<Vec<ResponseCheck>> = baml_value_with_meta
        .map_meta(|cs| {
            cs.iter()
                .map(|(label, expr, result)| {
                    let status = (if *result { "succeeded" } else { "failed" }).to_string();
                    ResponseCheck {
                        name: label.clone(),
                        expression: expr.0.clone(),
                        status,
                    }
                })
                .collect()
        });

    let baml_value_with_streaming =
        validate_streaming_state(ir, &baml_value, field_type, allow_partials)
            .map_err(|s| anyhow::anyhow!("{s:?}"))?;

    // Combine the baml_value, its types, the parser flags, and the streaming state
    // into a final value.
    // Node that we set the StreamState to `None` unless `allow_partials`.
    let response_value = baml_value_with_streaming
        .zip_meta(&value_with_response_checks)?
        .zip_meta(&meta_flags)?
        .map_meta(|((x, y), z)| (z.clone(), y.clone(), x.clone()));
    Ok(ResponseBamlValue(response_value))
}

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
    pub max_one_system_prompt: bool,
    pub resolve_media_urls: ResolveMediaUrls,
    pub allowed_metadata: AllowedRoleMetadata,
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
    pub request_options: BamlMap<String, serde_json::Value>,
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

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::InvalidAuthentication => f.write_str("InvalidAuthentication (401)"),
            ErrorCode::NotSupported => f.write_str("NotSupported (403)"),
            ErrorCode::RateLimited => f.write_str("RateLimited (429)"),
            ErrorCode::ServerError => f.write_str("ServerError (500)"),
            ErrorCode::ServiceUnavailable => f.write_str("ServiceUnavailable (503)"),
            ErrorCode::UnsupportedResponse(code) => write!(f, "BadResponse {code}"),
            ErrorCode::Other(code) => write!(f, "Unspecified error code: {code}"),
        }
    }
}

impl ErrorCode {
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
    pub request_options: BamlMap<String, serde_json::Value>,
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

// This is how the response gets logged if you print the result to the console.
// E.g. raw.__str__() in Python
impl std::fmt::Display for LLMCompleteResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}",
            format!(
                "Client: {} ({}) - {}ms. StopReason: {}. Tokens(in/out): {}/{}",
                self.client,
                self.model,
                self.latency.as_millis(),
                self.metadata.finish_reason.as_deref().unwrap_or("unknown"),
                self.metadata
                    .prompt_tokens
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                self.metadata
                    .output_tokens
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .yellow()
        )?;
        writeln!(f, "{}", "---PROMPT---".blue())?;
        writeln!(f, "{}", self.prompt.to_string().dimmed())?;
        writeln!(f, "{}", "---LLM REPLY---".blue())?;
        write!(f, "{}", self.content.dimmed())
    }
}

// This is the one that gets logged by BAML_LOG, for baml_events log.
impl crate::tracing::Visualize for LLMCompleteResponse {
    fn visualize(&self, max_chunk_size: usize) -> String {
        let s = [
            format!(
                "{}",
                format!(
                    "Client: {} ({}) - {}ms. StopReason: {}. Tokens(in/out): {}/{}",
                    self.client,
                    self.model,
                    self.latency.as_millis(),
                    self.metadata.finish_reason.as_deref().unwrap_or("unknown"),
                    self.metadata
                        .prompt_tokens
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    self.metadata
                        .output_tokens
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
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
        s.push(format!("{}", format!("---ERROR ({})---", self.code).red()));
        s.push(format!(
            "{}",
            crate::tracing::truncate_string(&self.message, max_chunk_size).red()
        ));
        s.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::{BamlValueWithMeta, FieldType};
    use internal_baml_core::ir::repr::{make_test_ir, IntermediateRepr};
    use jsonish::{
        deserializer::{deserialize_flags::DeserializerConditions, types::ValueWithFlags},
        BamlValueWithFlags,
    };

    fn mk_ir() -> IntermediateRepr {
        make_test_ir(
            r##"
        class Foo {
          i int
          s string @stream.done
        }
        "##,
        )
        .expect("Source is valid")
    }

    #[test]
    fn to_response() {
        let ir = mk_ir();
        let val = BamlValueWithFlags::Class(
            "Foo".to_string(),
            DeserializerConditions {
                flags: vec![Flag::Incomplete],
            },
            vec![
                (
                    "i".to_string(),
                    BamlValueWithFlags::Int(ValueWithFlags {
                        value: 1,
                        flags: DeserializerConditions { flags: Vec::new() },
                    }),
                ),
                (
                    "s".to_string(),
                    BamlValueWithFlags::String(ValueWithFlags {
                        value: "H".to_string(),
                        flags: DeserializerConditions {
                            flags: vec![Flag::Incomplete],
                        },
                    }),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let response = parsed_value_to_response(&ir, val, &FieldType::class("Foo"), true);
        assert!(response.is_ok());
    }

    fn mk_null() -> BamlValueWithFlags {
        BamlValueWithFlags::Null(DeserializerConditions::default())
    }

    fn mk_string(s: &str) -> BamlValueWithFlags {
        BamlValueWithFlags::String(ValueWithFlags {
            value: s.to_string(),
            flags: DeserializerConditions::default(),
        })
    }
    fn mk_float(s: f64) -> BamlValueWithFlags {
        BamlValueWithFlags::Float(ValueWithFlags {
            value: s,
            flags: DeserializerConditions::default(),
        })
    }

    #[test]
    fn stable_keys2() {
        let ir = make_test_ir(
            r##"
        class Address {
          street string
          state string
        }
        class Name {
          first string
          last string?
        }
        class Info {
          name Name
          address Address?
          hair_color string
          height float
        }
        "##,
        )
        .unwrap();

        let value = BamlValueWithFlags::Class(
            "Info".to_string(),
            DeserializerConditions::default(),
            vec![
                (
                    "name".to_string(),
                    BamlValueWithFlags::Class(
                        "Name".to_string(),
                        DeserializerConditions::default(),
                        vec![
                            ("first".to_string(), mk_string("Greg")),
                            ("last".to_string(), mk_string("Hale")),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
                ("address".to_string(), mk_null()),
                ("hair_color".to_string(), mk_string("Grey")),
                ("height".to_string(), mk_float(1.75)),
            ]
            .into_iter()
            .collect(),
        );
        let field_type = FieldType::class("Info");

        let res = parsed_value_to_response(&ir, value, &field_type, true).unwrap();

        let json = serde_json::to_value(res.serialize_final()).unwrap();

        match &json {
            serde_json::Value::Object(items) => {
                let (k, _) = items.iter().next().unwrap();
                assert_eq!(k, "name")
            }
            _ => panic!("Expected json object"),
        }
    }
}
