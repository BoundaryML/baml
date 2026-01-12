//! BAML Runtime - Core execution engine for BAML functions.
//!
//! This crate provides the runtime infrastructure for executing BAML functions,
//! including prompt rendering, LLM communication, and response parsing.
//!
//! # Architecture
//!
//! The runtime follows a pipeline architecture:
//!
//! 1. `prepare_function()` - Validates inputs and prepares execution context
//! 2. `render_prompt()` - Renders the Jinja template to provider-agnostic messages
//! 3. `build_provider_request()` - Converts to provider-specific format (e.g., OpenAI)
//! 4. `resolve_media()` - Resolves media URLs to inline data if needed
//! 5. `execute()` - Sends HTTP request to LLM provider
//! 6. `parse_response()` - Parses provider response to unified format
//! 7. `parse_output()` - Parses LLM content to BAML types
//!
//! # Feature Flags
//!
//! - `native`: Enables native async runtime with tokio and reqwest (default)
//! - `wasm`: Enables WASM-compatible async with wasm-bindgen-futures

pub mod api;
pub mod context;
pub mod errors;
pub mod function_lookup;
pub mod llm_request;
pub mod llm_response;
pub mod orchestrator;
pub mod parsing;

mod prepared_function;
mod render_options;
mod types;

pub use api::*;
pub use errors::*;
pub use prepared_function::*;
pub use render_options::*;
pub use types::*;

// Re-export ir_stub types for convenience
pub use ir_stub::{ClientSpec, FunctionDef, ParamDef, PromptTemplate, TypeRef};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_compiles() {
        // If this test runs, the crate structure is valid
        assert!(true);
    }

    #[test]
    fn test_render_options_default() {
        let opts = RenderOptions::default();
        assert!(!opts.expose_secrets);
        assert!(!opts.expand_media);
    }
}
