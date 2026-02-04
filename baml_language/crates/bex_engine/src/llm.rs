//! LLM-related `SysOp` implementations that read from heap Function objects.
//!
//! This module contains LLM operations that need access to function metadata:
//! - `get_jinja_template` - Look up the template for a function
//! - `get_client_function` - Get the client resolve function for a function
//!
//! Other LLM operations are in the `llm_ops` crate.

use bex_external_types::{BexExternalValue, BexValue};
use bex_vm_types::{FunctionMeta, HeapPtr, Object};
use sys_types::OpError;

/// Execute the `get_jinja_template` LLM operation.
///
/// Arguments: `[function_name: String]`
/// Returns: String (the Jinja template for the function's prompt)
pub(crate) fn execute_get_jinja_template(
    resolved_function_names: &std::collections::HashMap<
        String,
        (HeapPtr, bex_vm_types::FunctionKind),
    >,
    args: &[BexValue],
) -> Result<BexExternalValue, OpError> {
    // Extract function name
    let function_name = match &args[0] {
        BexValue::External(BexExternalValue::String(s)) => s.as_str(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: format!("{other:?}"),
            });
        }
    };

    // Look up function object
    let (ptr, _kind) = resolved_function_names
        .get(function_name)
        .ok_or_else(|| OpError::Other(format!("Function not found: {function_name}")))?;

    // SAFETY: ptr is from resolved_function_names, a compile-time object
    let obj = unsafe { ptr.get() };
    let Object::Function(func) = obj else {
        return Err(OpError::Other(format!("Not a function: {function_name}")));
    };

    // Extract prompt template from function metadata
    match &func.body_meta {
        Some(FunctionMeta::Llm {
            prompt_template, ..
        }) => Ok(BexExternalValue::String(prompt_template.clone())),
        _ => Err(OpError::Other(format!(
            "Function '{function_name}' is not an LLM function"
        ))),
    }
}

/// Execute the `get_client_function` LLM operation.
///
/// Arguments: `[function_name: String]`
/// Returns: `FunctionRef` (a callable reference to the client's resolve function)
///
/// This returns a function reference that, when called, evaluates the client's
/// options and returns a `PrimitiveClient`.
pub(crate) fn execute_get_client_function(
    resolved_function_names: &std::collections::HashMap<
        String,
        (HeapPtr, bex_vm_types::FunctionKind),
    >,
    function_global_indices: &std::collections::HashMap<String, usize>,
    args: &[BexValue],
) -> Result<BexExternalValue, OpError> {
    // Extract function name
    let function_name = match &args[0] {
        BexValue::External(BexExternalValue::String(s)) => s.as_str(),
        other => {
            return Err(OpError::TypeError {
                expected: "String",
                actual: format!("{other:?}"),
            });
        }
    };

    // Look up function object
    let (ptr, _kind) = resolved_function_names
        .get(function_name)
        .ok_or_else(|| OpError::Other(format!("Function not found: {function_name}")))?;

    // SAFETY: ptr is from resolved_function_names, a compile-time object
    let obj = unsafe { ptr.get() };
    let Object::Function(func) = obj else {
        return Err(OpError::Other(format!("Not a function: {function_name}")));
    };

    // Extract client name from function metadata
    let client_name = match &func.body_meta {
        Some(FunctionMeta::Llm { client, .. }) => client.as_str(),
        _ => {
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
