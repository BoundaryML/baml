//! Native function implementations for BAML builtins.
//!
//! Each sub-module implements one or more generated traits:
//!
//! - `array` — `BamlClassArray` (length, push, at, concat, ...)
//! - `float` — `BamlClassFloat` (predicates, rounding, math, trig, ...)
//! - `int` — `BamlClassInt` (abs, min, max, clamp, bit ops, ...)
//! - `string` — `BamlClassString` (length, trim, split, ...)
//! - `map` — `BamlClassMap` (length, has, keys, values, ...)
//! - `math` — `BamlNamespaceMath` (trunc)
//! - `media` — `BamlClassMedia{Pdf,Audio,Video,Image}` + `BamlNamespaceMedia`
//! - `ops` — `BamlClassOps*` (`Equals`/`Compare` for primitives + containers)
//! - `unstable` — `BamlNamespaceUnstable` (string)
//! - `root` — `BamlPackageBaml` (`deep_copy`, `deep_equals`, and the
//!   `Sortable.sort` shims `_compare_shim` / `_is_primitive_array` /
//!   `_rust_sort` / `_float_total_cmp`)
//!
//! # Adding a new builtin
//!
//! 1. Add the definition in the `.baml` stdlib under `crates/baml_builtins2/baml_std/`
//! 2. Implement the method in the appropriate sub-module's `impl` block

mod array;
pub(crate) mod bigint;
mod csv;
mod float;
mod future;
mod id;
mod int;
pub mod json;
mod map;
mod math;
mod media;
mod ops;
mod primitives;
mod resolve;
mod root;
mod spawn;
mod stack_trace;
mod string;
mod sys;
mod time;
mod toml;
mod type_class;
mod uint8array;
mod unstable;
mod yaml;

use std::collections::HashMap;

use bex_heap::TlabHolder;
use bex_vm_types::{
    ArrayReadGuard, HeapPtr, MapReadGuard,
    types::{Instance, Object, Type, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
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
    ///
    /// `type_args` carries explicit BEP-039 type arguments to seed the callee's
    /// frame.  This is the native counterpart of the `Call` instruction's
    /// type-arg channel — required so native helpers like `baml.json.from_json`
    /// can dispatch a generic class' `from_json` (e.g. `Box<Secret>.from_json`)
    /// with the right `T` substitution.  Pass `vec![]` for non-generic callees.
    YieldToCall {
        callee: HeapPtr,
        args: Vec<Value>,
        type_args: Vec<baml_type::RuntimeTy>,
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
// `unsafe_code` is intentional: float-boxed Object reads use `ptr.get()`
// which is unsafe; the surrounding accessors uphold the heap-permit
// contract (see `BexVm::get_object`).
#[allow(
    unused_variables,
    unsafe_code,
    // Synthetic `implement Interface for Type` names aren't strict Rust
    // casing: trait names like `BamlClassOpsEquals_for_int` aren't
    // UpperCamelCase, and dispatch methods like
    // `__dispatch_ops_equals_for_map_k__v_` (from `map<K, V>`) aren't snake_case.
    non_camel_case_types,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::iter_not_returning_iterator,
    clippy::needless_lifetimes,
    clippy::redundant_closure_call,
    // Static builtin constructors (e.g. `baml.spawn.CancelToken.new`) are
    // generated as trait methods returning `Value` (the heap instance), not
    // `Self` — that is the codegen contract, not a smell.
    clippy::new_ret_no_self,
    clippy::too_many_arguments,
    non_snake_case
)]
mod generated {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/nativefunctions_generated.rs"));
}
pub use generated::*;

/// The VM's native function implementations.
pub struct PackageBamlImpl;

// =============================================================================
// Shared helper: resolve `to_json` callee for a given value
// =============================================================================

/// For a given value `v`, look up the appropriate `to_json` function in the VM
/// globals, create a `BoundMethod { function: to_json_fn_ptr, receiver: v }`,
/// and return the `HeapPtr` to the bound method.
///
/// The bound method has `receiver = v` baked in; the VM inserts the receiver
/// as `self` when the bound method is dispatched, so `YieldToCall { args: [] }`
/// is correct (no extra arguments beyond self).
///
/// Used by both `Array.to_json` (in `array.rs`) and `Map.to_json` (in `map.rs`).
pub(super) fn make_to_json_callee(vm: &mut BexVm, v: Value) -> Result<HeapPtr, VmRustFnError> {
    use bex_vm_types::ValueKind;
    let fn_name: String = match v.kind() {
        ValueKind::Null => "baml.Null.to_json".to_string(),
        ValueKind::Bool(_) => "baml.Bool.to_json".to_string(),
        ValueKind::Int(_) => "baml.Int.to_json".to_string(),
        ValueKind::OmittedArg => {
            return Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "omitted argument cannot be converted to json".to_string(),
            }));
        }
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(_) => "baml.Float.to_json".to_string(),
            Object::String(_) => "baml.String.to_json".to_string(),
            Object::Array(_) => "baml.Array.to_json".to_string(),
            Object::Map(_) => "baml.Map.to_json".to_string(),
            Object::Instance(inst) => {
                let class_ptr = inst.class;
                let fqn = match vm.get_object(class_ptr) {
                    // Dispatch key must be the fully-qualified name (keeping the
                    // package), matching how functions are registered — not the
                    // user-facing `display_name` that elides `user`.
                    Object::Class(c) => c.name.render_dotted(false),
                    _ => {
                        return Err(VmRustFnError::InternalError(
                            VmInternalError::MissingNativeFunction {
                                name: "to_json dispatch: instance.class is not a Class".to_string(),
                            },
                        ));
                    }
                };
                format!("{fqn}.to_json")
            }
            _ => {
                return Err(VmRustFnError::InternalError(
                    VmInternalError::MissingNativeFunction {
                        name: "to_json dispatch: no to_json for this value type".to_string(),
                    },
                ));
            }
        },
    };

    let fn_ptr = vm.find_function_by_name(&fn_name).ok_or_else(|| {
        VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: format!("to_json dispatch: function '{fn_name}' not found in globals"),
        })
    })?;

    // Allocate a BoundMethod with the value as receiver.
    Ok(vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: fn_ptr,
        receiver: v,
    }))
}

/// For a value `v` whose type implements `baml.Comparable`, look up the
/// matching `compare` function and return a `BoundMethod { compare, receiver: v }`.
///
/// Builtin impls are out-of-body (`baml.Comparable$for$int.compare`, …); user
/// classes carry the in-body impl method `{class_fqn}.baml.Comparable.compare`.
/// The bound method has `receiver = v` baked in, so the VM inserts it as `self`
/// and the comparison call only passes the `other` argument
/// (`YieldToCall { args: [other] }`).
///
/// Used by the native `baml._compare_shim` (`root.rs`) that the BAML
/// `Sortable.sort` passes to `sort_by` on its non-primitive path: `compare`'s
/// two `Self` params make it undispatchable through an interface-typed value,
/// so the per-pair comparison is resolved here on the receiver's runtime class
/// (the homogeneous `T[]` guarantees the other element shares that class).
pub(super) fn make_compare_callee(vm: &mut BexVm, v: Value) -> Result<HeapPtr, VmRustFnError> {
    use bex_vm_types::ValueKind;
    let fn_name: String = match v.kind() {
        ValueKind::Int(_) => "baml.Comparable$for$int.compare".to_string(),
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(_) => "baml.Comparable$for$float.compare".to_string(),
            Object::String(_) => "baml.Comparable$for$string.compare".to_string(),
            Object::Bigint(_) => "baml.Comparable$for$bigint.compare".to_string(),
            Object::Instance(inst) => {
                let class_ptr = inst.class;
                let fqn = match vm.get_object(class_ptr) {
                    Object::Class(c) => c.name.render_dotted(false),
                    _ => {
                        return Err(VmRustFnError::InternalError(
                            VmInternalError::MissingNativeFunction {
                                name: "compare dispatch: instance.class is not a Class".to_string(),
                            },
                        ));
                    }
                };
                format!("{fqn}.baml.Comparable.compare")
            }
            _ => {
                return Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "_compare_shim: element type does not implement Comparable"
                        .to_string(),
                }));
            }
        },
        _ => {
            return Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "_compare_shim: element type does not implement Comparable".to_string(),
            }));
        }
    };

    let fn_ptr = vm.find_function_by_name(&fn_name).ok_or_else(|| {
        VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: format!("compare dispatch: function '{fn_name}' not found in globals"),
        })
    })?;

    Ok(vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: fn_ptr,
        receiver: v,
    }))
}

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
                return_type: function.return_type,
                param_names: function.param_names,
                param_types: function.param_types,
                param_has_default: function.param_has_default,
                display_type_params: function.display_type_params,
                display_param_types: function.display_param_types,
                display_return_type: function.display_return_type,
                throws_type: function.throws_type,
                origin: function.origin,
                body_meta: function.body_meta,
                function_id: 0, // synthetic; not in the profiling function table
            }))
        }
        other => other,
    })
}
