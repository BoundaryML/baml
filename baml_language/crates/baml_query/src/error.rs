use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, QueryError>;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("query {query_id} was cancelled")]
    Cancelled { query_id: uuid::Uuid },

    #[error("query is not read-only: {0}")]
    NotReadOnly(String),

    #[error("value {value_id} is missing at {path}")]
    MissingValue { value_id: String, path: PathBuf },

    #[error("value {value_id} is corrupt: {message}")]
    CorruptValue { value_id: String, message: String },

    #[error("value expansion exceeded the configured limit")]
    ValueLimit,

    #[error("value reference cycle detected at {0}")]
    ValueCycle(String),

    #[error("invalid value ID: {0}")]
    InvalidValueId(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("DataFusion error: {0}")]
    DataFusion(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<datafusion::error::DataFusionError> for QueryError {
    fn from(error: datafusion::error::DataFusionError) -> Self {
        Self::DataFusion(error.to_string())
    }
}
