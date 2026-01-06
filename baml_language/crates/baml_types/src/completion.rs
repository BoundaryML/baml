//! Completion state for streaming responses.

use serde::{Deserialize, Serialize};

/// The completion state of a value during streaming.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompletionState {
    /// The value is complete.
    Complete,
    /// The value is still being streamed.
    Incomplete,
    /// The value is pending (not yet started).
    Pending,
}

impl Default for CompletionState {
    fn default() -> Self {
        CompletionState::Complete
    }
}

impl std::fmt::Display for CompletionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompletionState::Complete => write!(f, "complete"),
            CompletionState::Incomplete => write!(f, "incomplete"),
            CompletionState::Pending => write!(f, "pending"),
        }
    }
}
