use axum::response::{IntoResponse, Response};
use http::StatusCode;
use internal_baml_core::ir::scope_diagnostics::ScopeStack;
use serde::Serialize;
use serde_json::json;

use crate::{errors::ExposedError, internal::llm_client::LLMResponse};

use super::json_response::Json;

/// The concrete HTTP error type that we return to our users if something goes wrong.
/// See https://docs.boundaryml.com/get-started/debugging/exception-handling for
/// an explanation of what each error variant is.
///
/// Variant names are deliberately redundant with the name of this enum: the variant
/// names are what we show to users, so it's important that they're self-evident.
#[derive(Debug, Serialize)]
#[serde(tag = "error", content = "message", rename_all = "snake_case")]
pub enum BamlError {
    InvalidArgument(String),
    ClientError(String),
    ValidationFailure(String),
    /// This is the only variant not documented at the aforementioned link:
    /// this is the catch-all for unclassified errors.
    InternalError(String),
}

impl BamlError {
    pub(crate) fn from_anyhow(err: anyhow::Error) -> Self {
        if let Some(er) = err.downcast_ref::<ExposedError>() {
            match er {
                ExposedError::ValidationError {
                    prompt,
                    raw_response,
                    message,
                } => Self::ValidationFailure(format!("{:?}", err)),
            }
        } else if let Some(er) = err.downcast_ref::<ScopeStack>() {
            Self::InvalidArgument(format!("{:?}", er))
        } else if let Some(er) = err.downcast_ref::<LLMResponse>() {
            match er {
                LLMResponse::Success(_) => {
                    Self::InternalError(format!("Unexpected error from BAML: {:?}", err))
                }
                LLMResponse::LLMFailure(failed) => match &failed.code {
                    crate::internal::llm_client::ErrorCode::Other(2) => Self::InternalError(
                        format!("Something went wrong with the LLM client: {:?}", err),
                    ),
                    crate::internal::llm_client::ErrorCode::Other(_)
                    | crate::internal::llm_client::ErrorCode::InvalidAuthentication
                    | crate::internal::llm_client::ErrorCode::NotSupported
                    | crate::internal::llm_client::ErrorCode::RateLimited
                    | crate::internal::llm_client::ErrorCode::ServerError
                    | crate::internal::llm_client::ErrorCode::ServiceUnavailable
                    | crate::internal::llm_client::ErrorCode::UnsupportedResponse(_) => {
                        Self::ClientError(format!("{:?}", err))
                    }
                },
                LLMResponse::UserFailure(msg) => {
                    Self::InvalidArgument(format!("Invalid argument: {}", msg))
                }
                LLMResponse::InternalFailure(_) => Self::InternalError(format!(
                    "Something went wrong with the LLM client: {}",
                    err
                )),
            }
        } else {
            Self::InternalError(format!("{:?}", err))
        }
    }
}

impl IntoResponse for BamlError {
    fn into_response(self) -> Response {
        (
            match &self {
                BamlError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
                BamlError::ClientError(_) => StatusCode::BAD_GATEWAY,
                BamlError::ValidationFailure(_) => StatusCode::INTERNAL_SERVER_ERROR, // ??? - FIXME
                BamlError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
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
