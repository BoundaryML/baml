//! This module implements parsing JSON-like data into a structured representation.
//!
//! The main entry point is the [`parse`] function, which takes a string and returns a [`Value`].
//! This is basically the jsonish equivalent of [`serde_json::from_str`] and [`serde_json::Value`].

mod parser;
mod value;

pub use parser::{ParseOptions, parse};
pub use value::{CompletionState, Fixes, Value};

/// Error type for jsonish parsing failures.
#[derive(Debug, thiserror::Error)]
pub enum JsonishError {
    #[error("Depth limit reached. Likely a circular reference.")]
    DepthLimitReached,
    #[error("No JSON objects found")]
    NoJsonObjectsFound,
    #[error("No markdown blocks found")]
    NoMarkdownBlocksFound,
    #[error("Mismatched brackets")]
    MismatchedBrackets,
    #[error("No collection to consume token: {0:?}")]
    NoCollectionForToken(char),
    #[error("Failed to parse JSON")]
    ParseFailed,
}
