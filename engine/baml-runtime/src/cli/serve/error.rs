use axum::response::{IntoResponse, Response};
use http::StatusCode;
use internal_baml_core::ir::scope_diagnostics::ScopeStack;
use serde::Serialize;
use serde_json::json;

use super::json_response::Json;
use crate::{errors::ExposedError, internal::llm_client::LLMResponse};

/// The concrete HTTP error type that we return to our users if something goes wrong.
/// See https://docs.boundaryml.com/get-started/debugging/exception-handling for
/// an explanation of what each error variant is.
///
/// Variant names are deliberately redundant with the name of this enum: the variant
/// names are what we show to users, so it's important that they're self-evident.
#[derive(Debug, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum BamlError {
    #[serde(rename_all = "snake_case")]
    InvalidArgument { message: String },
    #[serde(rename_all = "snake_case")]
    ClientError { message: String },
    #[serde(rename_all = "snake_case")]
    ValidationFailure {
        prompt: String,
        raw_output: String,
        message: String,
    },
    #[serde(rename_all = "snake_case")]
    FinishReasonError {
        prompt: String,
        raw_output: String,
        message: String,
        finish_reason: Option<String>,
    },
    #[serde(rename_all = "snake_case")]
    ClientHttpError {
        client_name: String,
        message: String,
        status_code: u16,
    },
    /// This is the only variant not documented at the aforementioned link:
    /// this is the catch-all for unclassified errors.
    #[serde(rename_all = "snake_case")]
    InternalError { message: String },
}

impl BamlError {
    pub(crate) fn from_anyhow(err: anyhow::Error) -> Self {
        if let Some(er) = err.downcast_ref::<ExposedError>() {
            match er {
                ExposedError::ValidationError {
                    prompt,
                    raw_output,
                    message,
                    ..
                } => Self::ValidationFailure {
                    prompt: prompt.to_string(),
                    raw_output: raw_output.to_string(),
                    message: message.to_string(),
                },
                ExposedError::FinishReasonError {
                    prompt,
                    raw_output,
                    message,
                    finish_reason,
                    ..
                } => Self::FinishReasonError {
                    prompt: prompt.to_string(),
                    raw_output: raw_output.to_string(),
                    message: message.to_string(),
                    finish_reason: finish_reason.clone(),
                },
                ExposedError::ClientHttpError {
                    client_name,
                    message,
                    status_code,
                    ..
                } => Self::ClientHttpError {
                    client_name: client_name.to_string(),
                    message: message.to_string(),
                    status_code: status_code.to_u16(),
                },
                ExposedError::TimeoutError {
                    client_name,
                    message,
                } => Self::ClientHttpError {
                    client_name: client_name.to_string(),
                    message: message.to_string(),
                    status_code: 408, // HTTP 408 Request Timeout
                },
                ExposedError::AbortError { .. } => Self::InternalError {
                    message: "AbortError".into(),
                },
            }
        } else if let Some(er) = err.downcast_ref::<ScopeStack>() {
            Self::InvalidArgument {
                message: format!("{er:?}"),
            }
        } else if let Some(er) = err.downcast_ref::<LLMResponse>() {
            match er {
                LLMResponse::Success(_) => Self::InternalError {
                    message: format!("Unexpected error from BAML: {err:?}"),
                },
                LLMResponse::LLMFailure(failed) => match &failed.code {
                    crate::internal::llm_client::ErrorCode::Other(2) => Self::InternalError {
                        message: format!("Something went wrong with the LLM client: {err:?}"),
                    },
                    crate::internal::llm_client::ErrorCode::Other(_)
                    | crate::internal::llm_client::ErrorCode::InvalidAuthentication
                    | crate::internal::llm_client::ErrorCode::NotSupported
                    | crate::internal::llm_client::ErrorCode::RateLimited
                    | crate::internal::llm_client::ErrorCode::ServerError
                    | crate::internal::llm_client::ErrorCode::ServiceUnavailable
                    | crate::internal::llm_client::ErrorCode::UnsupportedResponse(_)
                    | crate::internal::llm_client::ErrorCode::Timeout => Self::ClientError {
                        message: format!("{err:?}"),
                    },
                },
                LLMResponse::UserFailure(msg) => Self::InvalidArgument {
                    message: format!("Invalid argument: {msg}"),
                },
                LLMResponse::InternalFailure(_) => Self::InternalError {
                    message: format!("Something went wrong with the LLM client: {err}"),
                },
                LLMResponse::Cancelled(msg) => Self::InternalError {
                    message: format!("Operation cancelled: {msg}"),
                },
            }
        } else {
            Self::InternalError {
                message: format!("{err:?}"),
            }
        }
    }
}

impl IntoResponse for BamlError {
    fn into_response(self) -> Response {
        (
            match &self {
                BamlError::InvalidArgument { .. } => StatusCode::BAD_REQUEST,
                BamlError::ClientError { .. } => StatusCode::BAD_GATEWAY,
                BamlError::FinishReasonError { .. } => StatusCode::INTERNAL_SERVER_ERROR, // ??? - FIXME
                BamlError::ValidationFailure { .. } => StatusCode::INTERNAL_SERVER_ERROR, // ??? - FIXME
                BamlError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
                BamlError::ClientHttpError { status_code, .. } => {
                    StatusCode::from_u16(*status_code).unwrap_or(StatusCode::BAD_GATEWAY)
                }
            },
            Json(match serde_json::to_value(&self) {
                Ok(serde_json::Value::Object(mut v)) => {
                    v.insert(
                        "documentation_url".into(),
                        "https://docs.boundaryml.com/get-started/debugging/exception-handling"
                            .into(),
                    );
                    serde_json::Value::Object(v)
                }
                // These arms should never happen: BamlValue -> serde_json::Value should always succeed.
                Ok(v) => v,
                Err(e) => json!({
                    "error": format!("error serializing {e:?} {:?}", self),
                }),
            }),
        )
            .into_response()
    }
}
