//! LLM operations and prompt specialization.
//!
//! This crate provides:
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `SysOp` implementations for LLM operations (`render_prompt`, `build_primitive_client`, etc.)

mod model_features;
mod transformations;

use bex_external_types::{BexExternalValue, BexValue, PrimitiveClientValue, PromptAst};
pub use model_features::{AllowedMetadata, ModelFeatures};
use sys_types::OpError;

/// Specialize a prompt for a specific provider.
///
/// Applies three transformations in order:
/// 1. Merge adjacent same-role messages
/// 2. Consolidate system prompts (when `max_one_system_prompt` is true)
/// 3. Filter role metadata (strip disallowed metadata keys)
pub fn specialize_prompt(client: &PrimitiveClientValue, prompt: PromptAst) -> PromptAst {
    let features = ModelFeatures::for_provider(&client.provider, &client.options);

    let prompt = transformations::merge_adjacent_messages(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);
    transformations::filter_metadata(prompt, &features)
}

// ============================================================================
// SysOp Implementations
// ============================================================================

/// Execute the `render_prompt` LLM operation.
///
/// Arguments: `[PrimitiveClient, template: String, args: Map<String, Any>]`
pub fn execute_render_prompt(args: &[BexValue]) -> Result<PromptAst, OpError> {
    use bex_jinja_runtime::RenderPromptError;
    let BexValue::External(BexExternalValue::PrimitiveClient(client)) = &args[0] else {
        return Err(RenderPromptError::InvalidArgument {
            message: "expected PrimitiveClient, got something else".to_string(),
        }
        .into());
    };

    // Extract template string
    let BexValue::External(BexExternalValue::String(s)) = &args[1] else {
        return Err(RenderPromptError::InvalidArgument {
            message: "expected String for template, got something else".to_string(),
        }
        .into());
    };
    let template = s.as_str();

    // Extract args map
    let BexValue::External(BexExternalValue::Map { entries, .. }) = &args[2] else {
        return Err(RenderPromptError::InvalidArgument {
            message: "expected Map for args, got something else".to_string(),
        }
        .into());
    };
    let template_args = entries.clone();

    // Build render context from PrimitiveClient
    let render_ctx = bex_jinja_runtime::RenderContext {
        client: bex_jinja_runtime::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        // TODO: output_format should come from somewhere (function return type?)
        output_format: bex_llm_types::OutputFormatContent::new(bex_external_types::Ty::String),
        tags: indexmap::IndexMap::new(),
        // TODO: enums should be passed from the orchestrator or looked up from snapshot
        enums: std::collections::HashMap::new(),
    };

    // Call the Jinja runtime - RenderPromptError converts to OpError via From
    let vm_prompt_ast = bex_jinja_runtime::render_prompt(template, &template_args, &render_ctx)?;

    // Convert VM PromptAst to external PromptAst
    Ok(vm_prompt_ast_to_external(&vm_prompt_ast))
}

/// Execute the `specialize_prompt` LLM `SysOp`.
///
/// Arguments: `[PrimitiveClient, prompt: PromptAst]`
pub fn execute_specialize_prompt(args: &[BexValue]) -> Result<PromptAst, OpError> {
    let BexValue::External(BexExternalValue::PrimitiveClient(client)) = &args[0] else {
        return Err(bex_jinja_runtime::RenderPromptError::InvalidArgument {
            message: "expected PrimitiveClient, got something else".to_string(),
        }
        .into());
    };

    let BexValue::External(BexExternalValue::PromptAst(prompt)) = &args[1] else {
        return Err(bex_jinja_runtime::RenderPromptError::InvalidArgument {
            message: "expected PromptAst, got something else".to_string(),
        }
        .into());
    };

    Ok(specialize_prompt(client, prompt.clone()))
}

/// Convert VM `bex_vm_types::PromptAst` to external `bex_external_types::PromptAst`.
pub fn vm_prompt_ast_to_external(ast: &bex_vm_types::PromptAst) -> PromptAst {
    match ast {
        bex_vm_types::PromptAst::String(s) => PromptAst::String(s.clone()),
        bex_vm_types::PromptAst::Media(handle) => PromptAst::Media(*handle),
        bex_vm_types::PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let ext_content = vm_prompt_ast_to_external(content);
            // For now, just convert metadata to BexExternalValue::Null
            // In a full implementation, we'd need to convert the Value properly
            let ext_metadata = match metadata {
                bex_vm_types::Value::Null => BexExternalValue::Null,
                _ => BexExternalValue::Null, // TODO: proper conversion
            };
            PromptAst::Message {
                role: role.clone(),
                content: Box::new(ext_content),
                metadata: Box::new(ext_metadata),
            }
        }
        bex_vm_types::PromptAst::Vec(items) => {
            let ext_items: Vec<_> = items.iter().map(vm_prompt_ast_to_external).collect();
            PromptAst::Vec(ext_items)
        }
    }
}

/// Execute the `build_primitive_client` LLM operation.
///
/// Arguments: `[name: String, provider: String, default_role: String, allowed_roles: Vec<String>, options: Map]`
/// Returns: `PrimitiveClient`
///
/// This is a simple constructor that takes already-evaluated values and builds a `PrimitiveClient`.
/// Called from bytecode after the `options()` function has been evaluated.
pub fn execute_build_primitive_client(args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    // Extract name
    let name = match &args[0] {
        BexValue::External(BexExternalValue::String(s)) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: format!("{other:?}"),
            });
        }
    };

    // Extract provider
    let provider = match &args[1] {
        BexValue::External(BexExternalValue::String(s)) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: format!("{other:?}"),
            });
        }
    };

    // Extract default_role
    let default_role = match &args[2] {
        BexValue::External(BexExternalValue::String(s)) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: format!("{other:?}"),
            });
        }
    };

    // Extract allowed_roles
    let allowed_roles = match &args[3] {
        BexValue::External(BexExternalValue::Array { items, .. }) => items
            .iter()
            .map(|item| match item {
                BexExternalValue::String(s) => Ok(s.clone()),
                other => Err(OpError::TypeError {
                    expected: "String",
                    actual: other.type_name().to_string(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?,
        other => {
            return Err(OpError::TypeError {
                expected: "Array<String>",
                actual: format!("{other:?}"),
            });
        }
    };

    // Extract options map
    let options = match &args[4] {
        BexValue::External(BexExternalValue::Map { entries, .. }) => entries.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "Map",
                actual: format!("{other:?}"),
            });
        }
    };

    // Build the PrimitiveClient
    let client = PrimitiveClientValue {
        name,
        provider,
        default_role,
        allowed_roles,
        options,
    };

    Ok(BexExternalValue::PrimitiveClient(client))
}

/// Execute the `build_request` LLM operation.
///
/// Arguments: `[PrimitiveClient, prompt: PromptAst]`
/// Returns: `HttpRequest` (to be sent via HTTP)
///
/// TODO: Implement this by porting logic from legacy `LLMPrimitiveProvider::build_request`.
pub fn execute_build_request(_args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    panic!("LlmBuildRequest SysOp not yet implemented - TODO: port from legacy")
}

/// Execute the `parse` LLM operation.
///
/// Arguments: `[PrimitiveClient, response: Response, function_name: String]`
/// Returns: The parsed BAML value
///
/// TODO: Implement this by porting logic from legacy response parsing.
pub fn execute_parse_response(_args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    panic!("LlmParseResponse SysOp not yet implemented - TODO: port from legacy")
}

/// Execute the `http.send` operation.
///
/// Arguments: `[request: Request]`
/// Returns: `Response`
///
/// TODO: Implement this by extending the HTTP client to support full requests.
pub fn execute_http_send(_args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    panic!("HttpSend SysOp not yet implemented - TODO: implement HTTP POST support")
}
