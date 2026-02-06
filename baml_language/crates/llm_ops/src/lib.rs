//! LLM operations and prompt specialization.
//!
//! This crate provides:
//! - `specialize_prompt()` - Transform a generic `PromptAst` for a specific LLM provider
//! - `SysOp` implementations for LLM operations (`render_prompt`, `build_primitive_client`, etc.)

mod build_request;
mod model_features;
pub mod parse_response;
mod provider;
mod render_prompt;
mod specialize_prompt;

use std::sync::Arc;

use bex_external_types::BexExternalValue;
use bex_heap::{BexHeap, builtin_types};
use bex_vm_types::{HeapPtr, Object};
pub use build_request::PrimitiveClientValue;
pub use model_features::{AllowedMetadata, ModelFeatures};
pub use parse_response::{
    FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage, parse_response,
};
pub use provider::LlmProvider;
pub use render_prompt::execute_render_prompt;
pub use specialize_prompt::execute_specialize_prompt;
use sys_types::OpErrorKind;

// ============================================================================
// SysOp Implementations
// ============================================================================

/// Execute the `build_primitive_client` LLM operation.
///
/// Arguments: `[name: String, provider: String, default_role: String, allowed_roles: Vec<String>, options: Map]`
/// Returns: `Instance { class_name: "baml.llm.PrimitiveClient", fields }`
pub fn execute_build_primitive_client(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 5 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 5,
            actual: args.len(),
        });
    }

    let arg4 = args.pop().expect("len is 5");
    let arg3 = args.pop().expect("len is 4");
    let arg2 = args.pop().expect("len is 3");
    let arg1 = args.pop().expect("len is 2");
    let arg0 = args.pop().expect("len is 1");

    let (name, provider, default_role, allowed_roles, options) = heap
        .with_gc_protection(|protected| {
            let name = arg0.as_string(&protected).cloned()?;
            let provider = arg1.as_string(&protected).cloned()?;
            let default_role = arg2.as_string(&protected).cloned()?;
            let allowed_roles_ext = arg3.as_owned_but_very_slow(&protected)?;
            let allowed_roles = match &allowed_roles_ext {
                BexExternalValue::Array { items, .. } => items
                    .iter()
                    .map(|v| match v {
                        BexExternalValue::String(s) => Ok(s.clone()),
                        _ => Err(bex_heap::AccessError::TypeMismatch {
                            expected: "string",
                            actual: v.type_name().to_string(),
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(bex_heap::AccessError::TypeMismatch {
                        expected: "array",
                        actual: allowed_roles_ext.type_name().to_string(),
                    });
                }
            };
            let options_ext = arg4.as_owned_but_very_slow(&protected)?;
            let BexExternalValue::Map {
                entries: options, ..
            } = options_ext
            else {
                return Err(bex_heap::AccessError::TypeMismatch {
                    expected: "map",
                    actual: options_ext.type_name().to_string(),
                });
            };
            Ok::<_, bex_heap::AccessError>((name, provider, default_role, allowed_roles, options))
        })
        .map_err(OpErrorKind::AccessError)?;

    let client = PrimitiveClientValue {
        name,
        provider,
        default_role,
        allowed_roles,
        options,
    };

    // Return as Instance so it can be passed to execute_build_request via as_builtin_class
    Ok(primitive_client_value_to_instance(&client))
}

fn primitive_client_value_to_instance(client: &PrimitiveClientValue) -> BexExternalValue {
    let allowed_roles = BexExternalValue::Array {
        element_type: bex_external_types::Ty::String,
        items: client
            .allowed_roles
            .iter()
            .map(|s| BexExternalValue::String(s.clone()))
            .collect(),
    };
    let options = BexExternalValue::Map {
        key_type: bex_external_types::Ty::String,
        value_type: bex_external_types::Ty::String,
        entries: client.options.clone(),
    };
    BexExternalValue::Instance {
        class_name: "baml.llm.PrimitiveClient".to_string(),
        fields: indexmap::indexmap! {
            "name".to_string() => BexExternalValue::String(client.name.clone()),
            "provider".to_string() => BexExternalValue::String(client.provider.clone()),
            "default_role".to_string() => BexExternalValue::String(client.default_role.clone()),
            "allowed_roles".to_string() => allowed_roles,
            "options".to_string() => options,
        },
    }
}

/// Execute the `build_request` LLM operation.
///
/// Arguments: `[PrimitiveClient, prompt: PromptAst]`
/// Returns: `Instance { class_name: "baml.http.Request", fields: { method, url, headers, body } }`
pub fn execute_build_request(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 2 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 2,
            actual: args.len(),
        });
    }

    let arg1 = args.pop().expect("len is 2");
    let arg0 = args.pop().expect("len is 1");

    let (client_owned, prompt) = heap
        .with_gc_protection(|protected| {
            let client_ref = arg0.as_builtin_class::<builtin_types::PrimitiveClient>(&protected)?;
            let client_owned = client_ref.into_owned(&protected)?;
            let prompt_ref = arg1.as_prompt_ast_owned(&protected)?;
            Ok::<_, bex_heap::AccessError>((client_owned, prompt_ref))
        })
        .map_err(OpErrorKind::AccessError)?;

    // build_request expects PrimitiveClientValue with options: IndexMap<String, BexExternalValue>
    let client = PrimitiveClientValue {
        name: client_owned.name,
        provider: client_owned.provider,
        default_role: client_owned.default_role,
        allowed_roles: client_owned.allowed_roles,
        options: client_owned
            .options
            .into_iter()
            .map(|(k, v)| (k, BexExternalValue::String(v)))
            .collect(),
    };

    build_request::build_request(&client, prompt).map_err(|e| OpErrorKind::Other(e.to_string()))
}

/// Execute the `parse` LLM operation.
///
/// Arguments: `[PrimitiveClient, response: String, function_name: String]`
/// Returns: The parsed BAML value
///
/// TODO: Implement this by porting logic from legacy response parsing.
pub fn execute_parse_response(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
    resolved_function_names: &std::collections::HashMap<
        String,
        (HeapPtr, bex_vm_types::FunctionKind),
    >,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 3 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 3,
            actual: args.len(),
        });
    }

    let arg2 = args.pop().expect("len is 3");
    let arg1 = args.pop().expect("len is 2");
    // arg0 is not used
    let _arg0 = args.pop().expect("len is 1");

    let (response, function_name, expected_return_type) = heap.with_gc_protection(|protected| {
        let response = arg1.as_string(&protected).cloned()?;
        let function_name = arg2.as_string(&protected).cloned()?;
        let (ptr, _kind) = resolved_function_names.get(&function_name).ok_or_else(|| {
            bex_heap::AccessError::FunctionNotFound {
                expected: function_name.clone(),
            }
        })?;
        #[allow(unsafe_code)]
        let obj = unsafe { ptr.get() };
        let Object::Function(func) = obj else {
            return Err(OpErrorKind::Other(format!(
                "Not a function: {function_name}"
            )));
        };

        Ok((response, function_name, func.return_type.clone()))
    })?;

    if expected_return_type != bex_program::Ty::String {
        return Err(OpErrorKind::NotImplemented {
            message: format!("Function {function_name} does not return a string"),
        });
    }

    Ok(BexExternalValue::String(response))
}
