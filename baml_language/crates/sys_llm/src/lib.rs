//! LLM operations and prompt specialization.
//!
//! This crate provides:
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `SysOp` implementations for LLM operations (`render_prompt`, `build_primitive_client`, etc.)

mod build_request;
mod model_features;
mod provider;
mod render_prompt;
mod specialize_prompt;

use bex_external_types::{BexExternalValue, BexValue, PrimitiveClientValue};
pub use model_features::{AllowedMetadata, ModelFeatures};
pub use provider::LlmProvider;
pub use render_prompt::{execute_render_prompt, vm_prompt_ast_to_external};
pub use specialize_prompt::{execute_specialize_prompt, specialize_prompt};
use sys_types::OpError;

// ============================================================================
// SysOp Implementations
// ============================================================================

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
/// Returns: `Instance { class_name: "baml.http.Request", fields: { method, url, headers, body } }`
pub fn execute_build_request(args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    let BexValue::External(BexExternalValue::PrimitiveClient(client)) = &args[0] else {
        return Err(OpError::TypeError {
            expected: "PrimitiveClient",
            actual: format!("{:?}", args[0]),
        });
    };

    let BexValue::External(BexExternalValue::PromptAst(prompt)) = &args[1] else {
        return Err(OpError::TypeError {
            expected: "PromptAst",
            actual: format!("{:?}", args[1]),
        });
    };

    build_request::build_request(client, prompt.clone()).map_err(|e| OpError::Other(e.to_string()))
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
