//! Native function implementations for BAML builtins.
//!
//! Each sub-module implements one or more generated traits:
//!
//! - `array` — `BamlClassArray` (length, push, at, concat, ...)
//! - `string` — `BamlClassString` (length, trim, split, ...)
//! - `map` — `BamlClassMap` (length, has, keys, values, ...)
//! - `math` — `BamlNamespaceMath` (trunc)
//! - `media` — `BamlClassMedia{Pdf,Audio,Video,Image}` + `BamlNamespaceMedia`
//! - `unstable` — `BamlNamespaceUnstable` (string)
//! - `root` — `BamlPackageBaml` (`deep_copy`, `deep_equals`)
//!
//! # Adding a new builtin
//!
//! 1. Add the definition in the `.baml` stdlib under `crates/baml_builtins2/baml_std/`
//! 2. Implement the method in the appropriate sub-module's `impl` block

mod array;
mod map;
mod math;
mod media;
mod root;
mod string;
mod uint8array;
mod unstable;

use bex_vm_types::types::{Instance, Object, Type, Value};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{InternalError, RuntimeError, VmError},
};

/// Result type for native functions.
pub type NativeFunctionResult = Result<Value, VmError>;

/// Native function type alias.
pub type NativeFunction = fn(&mut BexVm, &[Value]) -> NativeFunctionResult;

// Generate the BamlClass*/BamlNamespace*/BamlPackageBaml trait hierarchy.
#[allow(
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::needless_lifetimes
)]
mod generated {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/nativefunctions_generated.rs"));
}
pub use generated::*;

/// The VM's native function implementations.
pub struct PackageBamlImpl;

// =============================================================================
// Public module-level function wrappers
//
// `vm.rs` calls these as free functions in the `crate::native` module.
// They delegate to the generated glue methods on `VmNatives`.
// =============================================================================

/// Resolves native function pointers for unresolved native functions in objects.
pub fn attach_builtins(object: Object) -> Result<Object, VmError> {
    Ok(match object {
        Object::Function(function) => {
            let kind = match function.kind {
                bex_vm_types::FunctionKind::Bytecode => bex_vm_types::FunctionKind::Bytecode,
                bex_vm_types::FunctionKind::SysOp(op) => bex_vm_types::FunctionKind::SysOp(op),
                bex_vm_types::FunctionKind::NativeUnresolved => {
                    let Some(native_function) =
                        PackageBamlImpl::get_native_fn(function.name.as_str())
                    else {
                        return Err(VmError::RuntimeError(RuntimeError::Other(format!(
                            "Native function '{}' not found",
                            function.name
                        ))));
                    };
                    bex_vm_types::FunctionKind::Native(native_function as *const ())
                }
                bex_vm_types::FunctionKind::Native(ptr) => bex_vm_types::FunctionKind::Native(ptr),
            };
            Object::Function(Box::new(bex_vm_types::Function {
                name: function.name,
                arity: function.arity,
                real_local_count: function.real_local_count,
                bytecode: function.bytecode,
                kind,
                local_names: function.local_names,
                debug_locals: function.debug_locals,
                span: function.span,
                block_notifications: function.block_notifications,
                viz_nodes: function.viz_nodes,
                return_type: function.return_type,
                param_names: function.param_names,
                param_types: function.param_types,
                body_meta: function.body_meta,
                trace: function.trace,
            }))
        }
        other => other,
    })
}
