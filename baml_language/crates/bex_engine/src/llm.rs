//! LLM-related system operations.
//!
//! These functions implement the `baml.llm.*` builtin operations:
//! - `render_prompt` - Render a Jinja template with a `PrimitiveClient`
//! - `get_jinja_template` - Look up a function's prompt template
//! - `build_primitive_client` - Construct a `PrimitiveClient` from evaluated options
//! - `get_client_function` - Get a function reference to a client's resolve function

use std::collections::HashMap;

use bex_external_types::{BexExternalValue, PrimitiveClientValue};
use bex_program::BexProgram;
use bex_vm_types::Value;
use sys_types::OpError;

/// Execute the `render_prompt` LLM operation.
///
/// Arguments: [`PrimitiveClient`, template: String, args: Map<String, Any>]
/// Returns: `PromptAst`
pub(crate) fn render_prompt(
    args: &[BexExternalValue],
) -> Result<bex_external_types::PromptAst, OpError> {
    use bex_jinja_runtime::RenderPromptError;

    // Extract PrimitiveClient
    let client = match &args[0] {
        BexExternalValue::PrimitiveClient(c) => c,
        other => {
            return Err(RenderPromptError::InvalidArgument {
                message: format!("expected PrimitiveClient, got {}", other.type_name()),
            }
            .into());
        }
    };

    // Extract template string
    let template = match &args[1] {
        BexExternalValue::String(s) => s.as_str(),
        other => {
            return Err(RenderPromptError::InvalidArgument {
                message: format!("expected String for template, got {}", other.type_name()),
            }
            .into());
        }
    };

    // Extract args map
    let template_args = match &args[2] {
        BexExternalValue::Map { entries, .. } => entries.clone(),
        other => {
            return Err(RenderPromptError::InvalidArgument {
                message: format!("expected Map for args, got {}", other.type_name()),
            }
            .into());
        }
    };

    // Build render context from PrimitiveClient
    let render_ctx = bex_jinja_runtime::RenderContext {
        client: bex_jinja_runtime::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        // TODO: output_format should come from somewhere (function return type?)
        output_format: bex_llm_types::OutputFormatContent::new(bex_program::Ty::String),
        tags: indexmap::IndexMap::new(),
        // TODO: enums should be passed from the orchestrator or looked up from snapshot
        enums: std::collections::HashMap::new(),
    };

    // Call the Jinja runtime - RenderPromptError converts to OpError via From
    let vm_prompt_ast = bex_jinja_runtime::render_prompt(template, &template_args, &render_ctx)?;

    // Convert VM PromptAst to external PromptAst
    Ok(vm_prompt_ast_to_external(&vm_prompt_ast))
}

/// Convert VM `bex_vm_types::PromptAst` to external `bex_external_types::PromptAst`.
pub(crate) fn vm_prompt_ast_to_external(
    ast: &bex_vm_types::PromptAst,
) -> bex_external_types::PromptAst {
    match ast {
        bex_vm_types::PromptAst::String(s) => bex_external_types::PromptAst::String(s.clone()),
        bex_vm_types::PromptAst::Media(handle) => bex_external_types::PromptAst::Media(*handle),
        bex_vm_types::PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let ext_content = vm_prompt_ast_to_external(content);
            // For now, just convert metadata to BexExternalValue::Null
            // In a full implementation, we'd need to convert the Value properly
            let ext_metadata = match metadata {
                Value::Null => BexExternalValue::Null,
                _ => BexExternalValue::Null, // TODO: proper conversion
            };
            bex_external_types::PromptAst::Message {
                role: role.clone(),
                content: Box::new(ext_content),
                metadata: Box::new(ext_metadata),
            }
        }
        bex_vm_types::PromptAst::Vec(items) => {
            let ext_items: Vec<_> = items.iter().map(vm_prompt_ast_to_external).collect();
            bex_external_types::PromptAst::Vec(ext_items)
        }
    }
}

/// Execute the `get_jinja_template` LLM operation.
///
/// Arguments: [`function_name`: String]
/// Returns: String (the Jinja template for the function's prompt)
pub(crate) fn get_jinja_template(
    snapshot: &BexProgram,
    args: &[BexExternalValue],
) -> Result<BexExternalValue, OpError> {
    // Extract function name
    let function_name = match &args[0] {
        BexExternalValue::String(s) => s.as_str(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: other.type_name().to_string(),
            });
        }
    };

    // Look up function definition
    let function_def = snapshot
        .functions
        .get(function_name)
        .ok_or_else(|| OpError::Other(format!("Function not found: {function_name}")))?;

    // Extract prompt template from LLM function body
    match &function_def.body {
        bex_program::FunctionBody::Llm {
            prompt_template, ..
        } => Ok(BexExternalValue::String(prompt_template.clone())),
        bex_program::FunctionBody::Expr { .. } => Err(OpError::Other(format!(
            "Function '{function_name}' is not an LLM function"
        ))),
    }
}

/// Execute the `build_primitive_client` LLM operation.
///
/// Arguments: [name: String, provider: String, `default_role`: String, `allowed_roles`: Array<String>, options: Map]
/// Returns: `PrimitiveClient`
///
/// This is a simple constructor that takes already-evaluated values and builds a `PrimitiveClient`.
/// Called from bytecode after the `options()` function has been evaluated.
pub(crate) fn build_primitive_client(
    args: &[BexExternalValue],
) -> Result<BexExternalValue, OpError> {
    // Extract name
    let name = match &args[0] {
        BexExternalValue::String(s) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: other.type_name().to_string(),
            });
        }
    };

    // Extract provider
    let provider = match &args[1] {
        BexExternalValue::String(s) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: other.type_name().to_string(),
            });
        }
    };

    // Extract default_role
    let default_role = match &args[2] {
        BexExternalValue::String(s) => s.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: other.type_name().to_string(),
            });
        }
    };

    // Extract allowed_roles
    let allowed_roles = match &args[3] {
        BexExternalValue::Array { items, .. } => items
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
                actual: other.type_name().to_string(),
            });
        }
    };

    // Extract options map
    let options = match &args[4] {
        BexExternalValue::Map { entries, .. } => entries.clone(),
        other => {
            return Err(OpError::TypeError {
                expected: "Map",
                actual: other.type_name().to_string(),
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

/// Execute the `get_client_function` LLM operation.
///
/// Arguments: [`function_name`: String]
/// Returns: `FunctionRef` (a callable reference to the client's resolve function)
///
/// This returns a function reference that, when called, evaluates the client's
/// options and returns a `PrimitiveClient`.
pub(crate) fn get_client_function(
    snapshot: &BexProgram,
    function_global_indices: &HashMap<String, usize>,
    args: &[BexExternalValue],
) -> Result<BexExternalValue, OpError> {
    // Extract function name
    let function_name = match &args[0] {
        BexExternalValue::String(s) => s.as_str(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: other.type_name().to_string(),
            });
        }
    };

    // Look up function definition
    let function_def = snapshot
        .functions
        .get(function_name)
        .ok_or_else(|| OpError::Other(format!("Function not found: {function_name}")))?;

    // Extract client name from LLM function body
    let client_name = match &function_def.body {
        bex_program::FunctionBody::Llm { client, .. } => client.as_str(),
        bex_program::FunctionBody::Expr { .. } => {
            return Err(OpError::Other(format!(
                "Function '{function_name}' is not an LLM function"
            )));
        }
    };

    // Build the resolve function name
    let resolve_fn_name = format!("{client_name}.resolve");

    // Look up the global index for the resolve function
    let global_index = function_global_indices
        .get(&resolve_fn_name)
        .ok_or_else(|| {
            OpError::Other(format!(
                "Client resolve function not found: {resolve_fn_name}"
            ))
        })?;

    // Return a FunctionRef that can be called to get the PrimitiveClient
    Ok(BexExternalValue::FunctionRef {
        global_index: *global_index,
    })
}
