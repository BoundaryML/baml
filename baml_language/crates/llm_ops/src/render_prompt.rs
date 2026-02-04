//! Jinja template rendering to `PromptAst`.

use bex_external_types::{BexExternalValue, BexValue, PromptAst};
use sys_types::OpError;

/// Execute the `render_prompt` LLM operation.
///
/// Arguments: `[PrimitiveClient, template: String, args: Map<String, Any>]`
pub fn execute_render_prompt(args: &[BexValue]) -> Result<PromptAst, OpError> {
    use llm_jinja::RenderPromptError;
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
    let render_ctx = llm_jinja::RenderContext {
        client: llm_jinja::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        // TODO: output_format should come from somewhere (function return type?)
        output_format: llm_types::OutputFormatContent::new(bex_external_types::Ty::String),
        tags: indexmap::IndexMap::new(),
        // TODO: enums should be passed from the orchestrator or looked up from snapshot
        enums: std::collections::HashMap::new(),
    };

    // Call the Jinja runtime - RenderPromptError converts to OpError via From
    let vm_prompt_ast = llm_jinja::render_prompt(template, &template_args, &render_ctx)?;

    // Convert VM PromptAst to external PromptAst
    Ok(vm_prompt_ast_to_external(&vm_prompt_ast))
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
