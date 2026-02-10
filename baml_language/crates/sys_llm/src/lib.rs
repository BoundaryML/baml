//! LLM operations, prompt specialization, and template rendering.
//!
//! This crate consolidates all LLM-related functionality:
//! - `types` - Error types and output format schema types
//! - `jinja` - Jinja template rendering for BAML prompts
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `execute_*` entry points for trait-based dispatch from `sys_types`

mod build_request;
pub(crate) mod jinja;
mod model_features;
pub(crate) mod parse_response;
mod provider;
mod render_prompt;
mod specialize_prompt;
pub(crate) mod types;

use std::str::FromStr;

use bex_external_types::BexExternalValue;
use bex_heap::builtin_types;
// Used by bex_engine tests
pub use jinja::{
    OutputFormatContent, RenderContext, RenderContextClient, RenderEnum, RenderEnumVariant,
    RenderPromptError, preprocess_template, render_prompt,
};
// --- Crate-internal re-exports (used by submodules via `crate::`) ---
pub(crate) use model_features::{AllowedMetadata, ModelFeatures};
pub(crate) use provider::LlmProvider;
// --- Public API: only what sys_types and bex_engine tests actually use ---

// Used by sys_types (From<LlmOpError> for OpErrorKind)
pub use types::LlmOpError;

// ============================================================================
// Clean (owned-type) entry points for trait-based dispatch
// ============================================================================

/// Render a Jinja template given already-extracted owned types.
///
/// `args` is expected to be `BexExternalValue::Map { entries, .. }`.
pub fn execute_render_prompt_from_owned(
    client: &builtin_types::owned::LlmPrimitiveClient,
    template: &str,
    args: &BexExternalValue,
) -> Result<bex_vm_types::PromptAst, LlmOpError> {
    let BexExternalValue::Map {
        entries: template_args,
        ..
    } = args
    else {
        return Err(LlmOpError::TypeError {
            expected: "map",
            actual: args.type_name().to_string(),
        });
    };

    let render_ctx = jinja::RenderContext {
        client: jinja::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        output_format: types::OutputFormatContent::new(bex_external_types::Ty::String),
        tags: indexmap::IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let prompt_ast = jinja::render_prompt(template, template_args, &render_ctx)
        .map_err(|e| LlmOpError::RenderPrompt(e.to_string()))?;
    Ok(std::sync::Arc::new(prompt_ast))
}

/// Specialize a prompt for a provider given already-extracted owned types.
pub fn execute_specialize_prompt_from_owned(
    client: &builtin_types::owned::LlmPrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<bex_vm_types::PromptAst, LlmOpError> {
    Ok(specialize_prompt::specialize_prompt_from_owned(
        client, prompt,
    ))
}

/// Build an HTTP request from a prompt given already-extracted owned types.
pub fn execute_build_request_from_owned(
    client: &builtin_types::owned::LlmPrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<builtin_types::owned::HttpRequest, LlmOpError> {
    build_request::build_request(client, prompt).map_err(|e| LlmOpError::Other(e.to_string()))
}

/// Parse an LLM response and extract the return value given already-extracted owned types.
pub fn execute_parse_response_from_owned(
    client: &builtin_types::owned::LlmPrimitiveClient,
    response: &str,
    return_type: &baml_type::Ty,
) -> Result<bex_external_types::BexExternalValue, LlmOpError> {
    let response = parse_response::parse_response(
        LlmProvider::from_str(&client.provider)
            .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?,
        response,
    )
    .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;
    match return_type {
        baml_type::Ty::String => Ok(bex_external_types::BexExternalValue::String(
            response.content,
        )),
        _ => Err(LlmOpError::NotImplemented {
            message: format!("Unsupported return type: {return_type:?}"),
        }),
    }
}
