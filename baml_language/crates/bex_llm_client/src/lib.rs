//! BAML Execution Engine - LLM Client.
//!
//! This crate provides client functionality for render_prompt and build_request interactions.
//!
//! # Pipeline
//!
//! The typical flow for making an LLM request is:
//!
//! 1. Build a `PromptAst` from a BAML function
//! 2. Apply client transformations via [`specialize_prompt`]: default_role, allowed_roles, remap_roles
//! 3. Apply provider transformations via [`providers::build_request`] to convert to `HttpRequest`
//! 4. Execute the `HttpRequest` and parse the response

pub mod providers;
mod transform;

pub use providers::{build_request, ProviderError};
pub use transform::{specialize_prompt, TransformError};

#[cfg(test)]
mod snapshot_tests;
