//! This module implements parsing JSON-like data into a structured representation.
//!
//! The main entry point is the [`parse`] function, which takes a string and returns a [`Value`].
//! This is basically the JSONish equivalent of [`serde_json::from_str`] and [`serde_json::Value`].

mod parser;
mod value;

pub use parser::{ParseOptions, parse};
pub use value::{CompletionState, Fixes, Value};
