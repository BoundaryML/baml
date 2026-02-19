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

    if !is_finish_reason_allowed(&client.options, response.finish_reason_raw.as_deref()) {
        return Err(LlmOpError::ParseResponseError(format!(
            "Finish reason not allowed: {}",
            response.finish_reason_raw.as_deref().unwrap_or("unknown")
        )));
    }

    match return_type {
        baml_type::Ty::String => Ok(bex_external_types::BexExternalValue::String(
            response.content,
        )),
        _ => Err(LlmOpError::NotImplemented {
            message: format!("Unsupported return type: {return_type:?}"),
        }),
    }
}

fn is_finish_reason_allowed(
    options: &indexmap::IndexMap<String, bex_external_types::BexExternalValue>,
    reason: Option<&str>,
) -> bool {
    let allow = extract_string_list(options.get("finish_reason_allow_list"));
    let deny = extract_string_list(options.get("finish_reason_deny_list"));

    match (allow, deny) {
        (Some(allow_list), None) => match reason {
            None => true,
            Some(r) => allow_list.iter().any(|v| v.eq_ignore_ascii_case(r)),
        },
        (None, Some(deny_list)) => match reason {
            None => true,
            Some(r) => !deny_list.iter().any(|v| v.eq_ignore_ascii_case(r)),
        },
        _ => true,
    }
}

fn extract_string_list(
    value: Option<&bex_external_types::BexExternalValue>,
) -> Option<Vec<String>> {
    let bex_external_types::BexExternalValue::Array { items, .. } = value? else {
        return None;
    };

    Some(
        items
            .iter()
            .filter_map(|v| match v {
                bex_external_types::BexExternalValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use bex_external_types::BexExternalValue;
    use bex_heap::builtin_types::owned::LlmPrimitiveClient;

    use super::execute_parse_response_from_owned;

    fn make_client_with_options(
        options: indexmap::IndexMap<String, BexExternalValue>,
    ) -> LlmPrimitiveClient {
        LlmPrimitiveClient {
            name: "TestClient".to_string(),
            provider: "openai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string(), "assistant".to_string()],
            options,
        }
    }

    fn single_string_array(value: &str) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: baml_type::Ty::String,
            items: vec![BexExternalValue::String(value.to_string())],
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

        let mut allow_options = indexmap::IndexMap::new();
        allow_options.insert(
            "finish_reason_allow_list".to_string(),
            single_string_array("stop"),
        );
        let allow_client = make_client_with_options(allow_options);

        // "stop" is allowed.
        let allowed =
            execute_parse_response_from_owned(&allow_client, response_stop, &baml_type::Ty::String);
        assert!(allowed.is_ok());

        // "length" is rejected.
        let blocked = execute_parse_response_from_owned(
            &allow_client,
            response_length,
            &baml_type::Ty::String,
        );
        assert!(blocked.is_err());

        let mut deny_options = indexmap::IndexMap::new();
        deny_options.insert(
            "finish_reason_deny_list".to_string(),
            single_string_array("length"),
        );
        let deny_client = make_client_with_options(deny_options);

        // "length" is rejected by deny list.
        let denied = execute_parse_response_from_owned(
            &deny_client,
            response_length,
            &baml_type::Ty::String,
        );
        assert!(denied.is_err());
    }
}
