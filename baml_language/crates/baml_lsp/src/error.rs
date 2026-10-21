//! The one LSP error type and its JSON-RPC code table.

use std::path::PathBuf;

/// Every failure a request or notification handler can report.
///
/// Serialized to the wire exclusively through [`LspError::to_response_error`];
/// the legacy `-32001 UnknownErrorCode` is never emitted.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("{0}")]
    NotificationExtractError(lsp_server::ExtractError<lsp_server::Notification>),
    #[error("Notification not supported: {0}")]
    NotificationNotSupported(String),
    #[error("{0}")]
    RequestExtractError(lsp_server::ExtractError<lsp_server::Request>),
    #[error("Request not supported: {0}")]
    RequestNotSupported(String),
    #[error("Failed to serialize request result: {0}")]
    RequestSerializeError(serde_json::Error),
    /// The client's sink is gone; nothing more can be delivered.
    #[error("Client closed")]
    ClientClosed,
    /// Bounded transport backpressure (LSP `RequestFailed`, `-32803`).
    #[error("LSP outbound sink is saturated")]
    OutboundSaturated,
    /// A frame larger than the transport limit (LSP `RequestFailed`,
    /// `-32803`).
    #[error("LSP outbound frame exceeds the transport limit")]
    OutboundOversized,
    #[error("Invalid command arguments for command: {command}: {message}")]
    InvalidCommandArguments { command: String, message: String },
    #[error("File not found: {}", .0.display())]
    FileNotFound(PathBuf),
    #[error("Path is invalid: {}: {message}", path.display())]
    InvalidPath { path: PathBuf, message: String },
    /// The document's path is under no known source root.
    #[error("No source root contains {}", .0.display())]
    NoRootForPath(PathBuf),
    /// Cancellation claimed the response while the request was queued or
    /// running (LSP `RequestCanceled`, `-32800`).
    #[error("Request canceled: {0}")]
    RequestCanceled(String),
    /// The request's snapshot became stale under an applied source change
    /// (LSP `ContentModified`, `-32801`).
    #[error("Content modified: {0}")]
    ContentModified(String),
    /// A valid request that cannot be served right now (LSP `RequestFailed`,
    /// `-32803`).
    #[error("{0}")]
    RequestFailed(String),
    /// Violated invariants, panics, serialization (LSP `InternalError`,
    /// `-32603`).
    #[error("Internal error: {0}")]
    Internal(String),
    /// Malformed params, position, or range (LSP `InvalidParams`, `-32602`).
    #[error("Invalid params: {0}")]
    InvalidParams(String),
    /// A request before `initialize` completed (LSP `ServerNotInitialized`,
    /// `-32002`).
    #[error("Server not initialized: {0}")]
    ServerNotInitialized(String),
}

impl LspError {
    /// The one LSP error-code mapping.
    #[must_use]
    pub fn to_response_error(&self) -> lsp_server::ResponseError {
        use lsp_server::ErrorCode;
        let code = match self {
            LspError::NotificationExtractError(_)
            | LspError::RequestExtractError(_)
            | LspError::InvalidCommandArguments { .. }
            | LspError::InvalidParams(_) => ErrorCode::InvalidParams,
            LspError::NotificationNotSupported(_) | LspError::RequestNotSupported(_) => {
                ErrorCode::MethodNotFound
            }
            LspError::ServerNotInitialized(_) => ErrorCode::ServerNotInitialized,
            LspError::RequestCanceled(_) => ErrorCode::RequestCanceled,
            LspError::ContentModified(_) => ErrorCode::ContentModified,
            LspError::FileNotFound(_)
            | LspError::InvalidPath { .. }
            | LspError::NoRootForPath(_)
            | LspError::OutboundSaturated
            | LspError::OutboundOversized
            | LspError::RequestFailed(_) => ErrorCode::RequestFailed,
            LspError::RequestSerializeError(_) | LspError::ClientClosed | LspError::Internal(_) => {
                ErrorCode::InternalError
            }
        };
        lsp_server::ResponseError {
            code: code as i32,
            message: self.to_string(),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LspError;

    /// The legacy `-32001` must never reach the wire.
    #[test]
    fn no_error_maps_to_the_legacy_unknown_code() {
        const LEGACY_UNKNOWN: i32 = -32001;
        let samples = [
            LspError::RequestNotSupported("x".into()),
            LspError::ClientClosed,
            LspError::OutboundSaturated,
            LspError::OutboundOversized,
            LspError::FileNotFound("a".into()),
            LspError::NoRootForPath("a".into()),
            LspError::RequestCanceled("x".into()),
            LspError::ContentModified("x".into()),
            LspError::RequestFailed("x".into()),
            LspError::Internal("x".into()),
            LspError::InvalidParams("x".into()),
            LspError::ServerNotInitialized("x".into()),
        ];
        for e in samples {
            assert_ne!(e.to_response_error().code, LEGACY_UNKNOWN, "{e}");
        }
    }
}
