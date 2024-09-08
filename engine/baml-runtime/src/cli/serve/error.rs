use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::Serialize;
use serde_json::json;

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
