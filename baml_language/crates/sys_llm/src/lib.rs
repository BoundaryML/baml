//! LLM operations, prompt specialization, and template rendering.
//!
//! This crate consolidates all LLM-related functionality:
//! - `types` - Error types and output format schema types
//! - `jinja` - Jinja template rendering for BAML prompts
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `execute_*` entry points for trait-based dispatch from `sys_types`

pub mod baml_std;
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
    client: &baml_std::PrimitiveClient,
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
            default_role: client.default_role(),
            allowed_roles: client.allowed_roles(),
        },
        output_format: types::OutputFormatContent::new(bex_external_types::Ty::String {
            attr: baml_type::TyAttr::default(),
        }),
        tags: indexmap::IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let prompt_ast = jinja::render_prompt(template, template_args, &render_ctx)
        .map_err(|e| LlmOpError::RenderPrompt(e.to_string()))?;
    Ok(std::sync::Arc::new(prompt_ast))
}

/// Specialize a prompt for a provider given already-extracted owned types.
pub fn execute_specialize_prompt_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<bex_vm_types::PromptAst, LlmOpError> {
    Ok(specialize_prompt::specialize_prompt_from_owned(
        client, prompt,
    ))
}

/// Build an HTTP request from a prompt given already-extracted owned types.
pub fn execute_build_request_from_owned(
    client: &baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<baml_std::HttpRequest, LlmOpError> {
    build_request::build_request(client, prompt).map_err(|e| LlmOpError::Other(e.to_string()))
}

/// Parse an LLM response and extract the return value given already-extracted owned types.
pub fn execute_parse_response_from_owned(
    client: &baml_std::PrimitiveClient,
    response: &str,
    return_type: &baml_type::Ty,
) -> Result<bex_external_types::BexExternalValue, LlmOpError> {
    let response = parse_response::parse_response(
        LlmProvider::from_str(&client.provider)
            .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?,
        response,
    )
    .map_err(|e| LlmOpError::ParseResponseError(e.to_string()))?;

    if !client.is_finish_reason_allowed(response.finish_reason_raw.as_deref()) {
        return Err(LlmOpError::ParseResponseError(format!(
            "Finish reason not allowed: {}",
            response.finish_reason_raw.as_deref().unwrap_or("unknown")
        )));
    }

    match return_type {
        baml_type::Ty::String { .. } => Ok(bex_external_types::BexExternalValue::String(
            response.content,
        )),
        _ => Err(LlmOpError::NotImplemented {
            message: format!("Unsupported return type: {return_type:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use ::baml_base::TyAttr;

    use super::execute_parse_response_from_owned;
    use crate::baml_std;

    fn make_client_with_options(
        options: baml_std::PrimitiveClientOptions,
    ) -> baml_std::PrimitiveClient {
        baml_std::PrimitiveClient {
            name: "TestClient".to_string(),
            provider: "openai".to_string(),
            options,
        }
    }

    #[test]
    fn parse_respects_finish_reason_filters() {
        let response_stop = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        }"#;
        let response_length = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "truncated" },
                "finish_reason": "length"
            }]
        }"#;

        let allow_client = make_client_with_options(baml_std::PrimitiveClientOptions {
            allowed_roles_allow_list: Some(vec!["stop".to_string()]),
            ..Default::default()
        });

        // "stop" is allowed.
        let allowed = execute_parse_response_from_owned(
            &allow_client,
            response_stop,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
        );
        assert!(allowed.is_ok());

        // "length" is rejected.
        let blocked = execute_parse_response_from_owned(
            &allow_client,
            response_length,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
        );
        assert!(blocked.is_err());

        let deny_client = make_client_with_options(baml_std::PrimitiveClientOptions {
            allowed_roles_deny_list: Some(vec!["length".to_string()]),
            ..Default::default()
        });

        // "length" is rejected by deny list.
        let denied = execute_parse_response_from_owned(
            &deny_client,
            response_length,
            &::baml_type::Ty::String {
                attr: TyAttr::default(),
            },
        );
        assert!(denied.is_err());
    }
}
