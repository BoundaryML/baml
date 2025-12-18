/// BAML runtime errors
///
/// Note: This is intentionally minimal. Expand with specific variants
/// (InitError, CallError, etc.) once the core functionality works.
#[derive(Debug, thiserror::Error)]
pub enum BamlError {
    #[error("{0}")]
    Internal(String),
}

impl BamlError {
    pub fn internal(msg: impl Into<String>) -> Self {
        BamlError::Internal(msg.into())
    }
}
