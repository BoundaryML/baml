//! LLM-related `SysOp` implementations that require `BexProgram`.
//!
//! This module contains LLM operations that need access to the program snapshot:
//! - `get_jinja_template` - Look up the template for a function
//! - `get_client_function` - Get the client resolve function for a function
//!
//! Other LLM operations are in the `sys_llm` crate.

use bex_external_types::{BexExternalValue, BexValue};
use bex_program::BexProgram;
use sys_types::OpError;

/// Execute the `get_jinja_template` LLM operation.
///
/// Arguments: `[function_name: String]`
/// Returns: String (the Jinja template for the function's prompt)
pub(crate) fn execute_get_jinja_template(
    snapshot: &BexProgram,
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
        bex_program::FunctionBody::Expr => Err(OpError::Other(format!(
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
    snapshot: &BexProgram,
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

    // Look up function definition
    let function_def = snapshot
        .functions
        .get(function_name)
        .ok_or_else(|| OpError::Other(format!("Function not found: {function_name}")))?;

    // Extract client name from LLM function body
    let client_name = match &function_def.body {
        bex_program::FunctionBody::Llm { client, .. } => client.as_str(),
        bex_program::FunctionBody::Expr => {
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
