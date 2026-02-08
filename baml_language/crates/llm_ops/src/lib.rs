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
pub use bex_heap::builtin_types::owned::PrimitiveClient;
use bex_heap::{BexHeap, builtin_types};
pub use model_features::{AllowedMetadata, ModelFeatures};
pub use parse_response::{
    FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage, parse_response,
};
pub use provider::LlmProvider;
pub use render_prompt::execute_render_prompt;
pub use specialize_prompt::execute_specialize_prompt;
use sys_types::{FunctionRef, OpErrorKind, SysOpContext};

// ============================================================================
// Clean (owned-type) entry points for trait-based dispatch
// ============================================================================

/// Render a Jinja template given already-extracted owned types.
///
/// `args` is expected to be `BexExternalValue::Map { entries, .. }`.
pub fn execute_render_prompt_from_owned(
    client: &builtin_types::owned::PrimitiveClient,
    template: &str,
    args: &BexExternalValue,
) -> Result<bex_vm_types::PromptAst, OpErrorKind> {
    let template_args: indexmap::IndexMap<String, BexExternalValue> = match args {
        BexExternalValue::Map { entries, .. } => entries.clone(),
        _ => {
            return Err(OpErrorKind::TypeError {
                expected: "map",
                actual: args.type_name().to_string(),
            });
        }
    };

    let render_ctx = llm_jinja::RenderContext {
        client: llm_jinja::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles.clone(),
        },
        output_format: llm_types::OutputFormatContent::new(bex_external_types::Ty::String),
        tags: indexmap::IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let prompt_ast = llm_jinja::render_prompt(template, &template_args, &render_ctx)?;
    Ok(std::sync::Arc::new(prompt_ast))
}

/// Specialize a prompt for a provider given already-extracted owned types.
pub fn execute_specialize_prompt_from_owned(
    client: &builtin_types::owned::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<bex_vm_types::PromptAst, OpErrorKind> {
    Ok(specialize_prompt::specialize_prompt_from_owned(
        client, prompt,
    ))
}

/// Build an HTTP request from a prompt given already-extracted owned types.
pub fn execute_build_request_from_owned(
    client: &builtin_types::owned::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<builtin_types::owned::HttpRequest, OpErrorKind> {
    build_request::build_request(client, prompt).map_err(|e| OpErrorKind::Other(e.to_string()))
}

// ============================================================================
// SysOp Implementations (legacy fn-pointer wrappers)
// ============================================================================

/// SysOpFn-compatible wrapper for `render_prompt`.
pub fn sys_op_render_prompt(
    heap: &Arc<BexHeap>,
    args: Vec<bex_heap::BexValue<'_>>,
    _ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_render_prompt(heap, args)
            .map(|ast| BexExternalValue::Adt(bex_external_types::BexExternalAdt::PromptAst(ast)))
            .map_err(|e| {
                sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmPrimitiveClientRenderPrompt, e)
            }),
    )
}

/// SysOpFn-compatible wrapper for `specialize_prompt`.
pub fn sys_op_specialize_prompt(
    heap: &Arc<BexHeap>,
    args: Vec<bex_heap::BexValue<'_>>,
    _ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_specialize_prompt(heap, args)
            .map(|ast| BexExternalValue::Adt(bex_external_types::BexExternalAdt::PromptAst(ast)))
            .map_err(|e| {
                sys_types::OpError::new(
                    bex_vm_types::SysOp::BamlLlmPrimitiveClientSpecializePrompt,
                    e,
                )
            }),
    )
}

/// SysOpFn-compatible wrapper for `build_primitive_client`.
pub fn sys_op_build_primitive_client(
    heap: &Arc<BexHeap>,
    args: Vec<bex_heap::BexValue<'_>>,
    _ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_build_primitive_client(heap, args).map_err(|e| {
            sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmBuildPrimitiveClient, e)
        }),
    )
}

/// SysOpFn-compatible wrapper for `build_request`.
pub fn sys_op_build_request(
    heap: &Arc<BexHeap>,
    args: Vec<bex_heap::BexValue<'_>>,
    _ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(execute_build_request(heap, args).map_err(|e| {
        sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmPrimitiveClientBuildRequest, e)
    }))
}

/// SysOpFn-compatible wrapper for `get_jinja_template` (uses engine context).
pub fn sys_op_get_jinja_template(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_get_jinja_template(heap, &mut args, ctx)
            .map_err(|e| sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmGetJinjaTemplate, e)),
    )
}

/// SysOpFn-compatible wrapper for `get_client_function` (uses engine context).
pub fn sys_op_get_client_function(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_get_client_function(heap, &mut args, ctx)
            .map_err(|e| sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmGetClientFunction, e)),
    )
}

/// SysOpFn-compatible wrapper for `parse` (uses engine context).
pub fn sys_op_parse_response(
    heap: &Arc<BexHeap>,
    args: Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> sys_types::SysOpResult {
    sys_types::SysOpResult::Ready(
        execute_parse_response(heap, args, ctx).map_err(|e| {
            sys_types::OpError::new(bex_vm_types::SysOp::BamlLlmPrimitiveClientParse, e)
        }),
    )
}

// ============================================================================
// Engine-context ops (previously in bex_engine::llm)
// ============================================================================

/// Execute the `get_jinja_template` LLM operation.
///
/// Arguments: `[function_name: String]`
/// Returns: String (the Jinja template for the function's prompt)
fn execute_get_jinja_template(
    heap: &Arc<BexHeap>,
    args: &mut Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 1 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.pop().expect("len is 1");
    let function_name = heap.with_gc_protection(|protected| arg0.as_string(&protected).cloned())?;

    let info = ctx
        .llm_functions
        .get(&function_name)
        .ok_or_else(|| OpErrorKind::Other(format!("LLM function not found: {function_name}")))?;

    Ok(BexExternalValue::String(info.prompt_template.clone()))
}

/// Execute the `get_client_function` LLM operation.
///
/// Arguments: `[function_name: String]`
/// Returns: `FunctionRef` (a callable reference to the client's resolve function)
fn execute_get_client_function(
    heap: &Arc<BexHeap>,
    args: &mut Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 1 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.pop().expect("len is 1");
    let function_name = heap.with_gc_protection(|protected| arg0.as_string(&protected).cloned())?;

    let info = ctx
        .llm_functions
        .get(&function_name)
        .ok_or_else(|| OpErrorKind::Other(format!("LLM function not found: {function_name}")))?;

    let resolve_fn_name = format!("{}.resolve", info.client_name);

    let global_index = ctx
        .function_global_indices
        .get(&resolve_fn_name)
        .ok_or_else(|| {
            OpErrorKind::Other(format!(
                "Client resolve function not found: {resolve_fn_name}"
            ))
        })?;

    Ok(
        FunctionRef::<bex_heap::builtin_types::owned::PrimitiveClient>::new(*global_index)
            .into_external(),
    )
}

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

    let arg0 = args.remove(0);
    let arg1 = args.remove(0);
    let arg2 = args.remove(0);
    let arg3 = args.remove(0);
    let arg4 = args.remove(0);

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

    let client = builtin_types::owned::PrimitiveClient {
        name,
        provider,
        default_role,
        allowed_roles,
        options,
    };

    // Return as Instance so it can be passed to execute_build_request via as_builtin_class
    Ok(client.as_bex_external_value())
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

    let arg0 = args.remove(0);
    let arg1 = args.remove(0);

    let (client_owned, prompt) = heap
        .with_gc_protection(|protected| {
            let client_ref = arg0.as_builtin_class::<builtin_types::PrimitiveClient>(&protected)?;
            let client_owned = client_ref.into_owned(&protected)?;
            let prompt_ref = arg1.as_prompt_ast_owned(&protected)?;
            Ok::<_, bex_heap::AccessError>((client_owned, prompt_ref))
        })
        .map_err(OpErrorKind::AccessError)?;

    build_request::build_request(&client_owned, prompt)
        .map(Into::into)
        .map_err(|e| OpErrorKind::Other(e.to_string()))
}

/// Execute the `parse` LLM operation.
///
/// Arguments: `[PrimitiveClient, response: String, function_name: String]`
/// Returns: The parsed BAML value
///
/// TODO: Implement this by porting logic from legacy response parsing.
fn execute_parse_response(
    heap: &Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
    ctx: &SysOpContext,
) -> Result<BexExternalValue, OpErrorKind> {
    if args.len() != 3 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 3,
            actual: args.len(),
        });
    }

    let _arg0 = args.remove(0);
    let arg1 = args.remove(0);
    let arg2 = args.remove(0);

    let (response, function_name) = heap
        .with_gc_protection(|protected| {
            let response = arg1.as_string(&protected).cloned()?;
            let function_name = arg2.as_string(&protected).cloned()?;
            Ok::<_, bex_heap::AccessError>((response, function_name))
        })
        .map_err(OpErrorKind::AccessError)?;

    let info = ctx
        .llm_functions
        .get(&function_name)
        .ok_or_else(|| OpErrorKind::Other(format!("LLM function not found: {function_name}")))?;

    if info.return_type != bex_program::Ty::String {
        return Err(OpErrorKind::NotImplemented {
            message: format!("Function {function_name} does not return a string"),
        });
    }

    Ok(BexExternalValue::String(response))
}
