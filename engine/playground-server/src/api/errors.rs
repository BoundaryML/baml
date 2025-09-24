use anyhow::Error as AnyhowError;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug)]
pub struct HttpError(pub AnyhowError);

impl From<AnyhowError> for HttpError {
    fn from(err: AnyhowError) -> Self {
        HttpError(err)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> axum::response::Response {
        // For IPC, keep it simple but preserve error chain information
        let error_message = format!("{:#}", self.0);

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": error_message,
                "type": "InternalError"
            })),
        )
            .into_response()
    }
}

// Keep existing From implementation for backward compatibility
impl From<serde_json::Error> for HttpError {
    fn from(err: serde_json::Error) -> Self {
        HttpError(anyhow::anyhow!("JSON serialization error: {}", err))
    }
}

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    InternalError(String),
    Unauthorized(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::BadRequest(format!("JSON serialization error: {}", err))
    }
}
