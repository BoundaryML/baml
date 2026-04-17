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
mod events;
mod map;
mod math;
mod media;
mod root;
mod stack_trace;
mod string;
mod sys;
mod uint8array;
mod unstable;

use std::collections::HashMap;

use bex_vm_types::{
    HeapPtr,
    types::{Instance, Object, Type, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

/// Result type for native functions.
pub type NativeFunctionResult = Result<Value, VmRustFnError>;

/// Native function type alias.
pub type NativeFunction = fn(&mut BexVm, &[Value]) -> NativeCallResult;

/// Result returned by native functions. Non-yielding functions return `Done` or
/// `Error`; yielding functions (like `array.map`) may return `YieldToCall` to
/// invoke a bytecode callback via the CPS trampoline.
pub enum NativeCallResult {
    /// Native function completed successfully.
    Done(Value),
    /// Native function failed.
    Error(VmRustFnError),
    /// Yield control to call a bytecode function, then invoke the continuation
    /// with its return value.
    YieldToCall {
        callee: HeapPtr,
        args: Vec<Value>,
        continuation: Box<dyn Continuation>,
    },
}

impl From<VmRustFnError> for NativeCallResult {
    fn from(err: VmRustFnError) -> Self {
        NativeCallResult::Error(err)
    }
}

impl From<VmInternalError> for NativeCallResult {
    fn from(err: VmInternalError) -> Self {
        NativeCallResult::Error(VmRustFnError::InternalError(err))
    }
}

/// A continuation that resumes a native function after a bytecode callback returns.
/// Must expose GC roots so the compacting collector can find and forward `HeapPtr`
/// values captured by the continuation.
pub trait Continuation: Send {
    /// Invoke the continuation with the callback's return value.
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult;

    /// Return all `HeapPtr` values held by this continuation (for GC root scanning).
    fn gc_roots(&self) -> Vec<HeapPtr>;

    /// Update all `HeapPtr` values after GC moves objects (forwarding).
    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>);
}

// Generate the BamlClass*/BamlNamespace*/BamlPackageBaml trait hierarchy.
#[allow(
    unused_variables,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::needless_lifetimes,
    clippy::redundant_closure_call
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
///
/// Only functions in the `baml.*` namespace are resolved here. Functions from
/// other packages (e.g. `assert.*`, `testing.*`) are left as `NativeUnresolved`
/// so they can be wired up by future package implementations. They will only
/// fail at runtime if actually called.
pub fn attach_builtins(object: Object) -> Result<Object, VmInternalError> {
    Ok(match object {
        Object::Function(function) => {
            let kind = match function.kind {
                bex_vm_types::FunctionKind::Bytecode => bex_vm_types::FunctionKind::Bytecode,
                bex_vm_types::FunctionKind::SysOp(op) => bex_vm_types::FunctionKind::SysOp(op),
                bex_vm_types::FunctionKind::NativeUnresolved => {
                    // Only attempt resolution for the `baml.*` package. Functions
                    // from other stdlib packages (assert, testing, …) are deferred.
                    if !function.name.starts_with("baml.") {
                        bex_vm_types::FunctionKind::NativeUnresolved
                    } else {
                        let Some(native_function) =
                            PackageBamlImpl::get_native_fn(function.name.as_str())
                        else {
                            return Err(VmInternalError::MissingNativeFunction {
                                name: function.name.clone(),
                            });
                        };
                        bex_vm_types::FunctionKind::Native(native_function as *const ())
                    }
                }
                bex_vm_types::FunctionKind::Native(ptr) => bex_vm_types::FunctionKind::Native(ptr),
            };
            Object::Function(Box::new(bex_vm_types::Function {
                name: function.name,
                source_file: function.source_file,
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
                stream_return_type: function.stream_return_type,
                param_names: function.param_names,
                param_types: function.param_types,
                throws_type: function.throws_type,
                body_meta: function.body_meta,
                trace: function.trace,
            }))
        }
        other => other,
    })
}
