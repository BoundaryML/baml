//! Native function implementations for BAML builtins.
//!
//! Each sub-module implements one or more generated traits:
//!
//! - `array` — `BamlClassArray` (length, push, at, concat, ...)
//! - `crypto` — `BamlClassCrypto{Aes128,Aes256}GcmSiv` (AEAD seal/open)
//! - `float` — `BamlClassFloat` (predicates, rounding, math, trig, ...)
//! - `int` — `BamlClassInt` (abs, min, max, clamp, bit ops, ...)
//! - `string` — `BamlClassString` (length, trim, split, ...)
//! - `map` — `BamlClassMap` (length, has, keys, values, ...)
//! - `media` — `BamlClassMedia{Pdf,Audio,Video,Image}` + `BamlNamespaceMedia`
//! - `ops` — `BamlClassOps*` (`Equals`/`Compare` for primitives + containers)
//! - `ops_math` — `BamlClassOps*` (`Add`/`Subtract`/`Multiply`/`Divide`/
//!   `Remainder`/`Negate` for the numeric primitives)
//! - `root` — `BamlPackageBaml` (`deep_copy`, the numeric-array
//!   reductions `_sum_int` / `_sum_float` / `_mean_float` / `_median_float`,
//!   the saturating `_trunc_to_int`, and the `Sortable.sort` shims
//!   `_compare_shim` / `_is_primitive_array` / `_rust_sort` / `_float_total_cmp`)
//!
//! # Adding a new builtin
//!
//! 1. Add the definition in the `.baml` stdlib under `crates/baml_builtins2/baml_std/`
//! 2. Implement the method in the appropriate sub-module's `impl` block

mod array;
pub(crate) mod bigint;
mod crypto;
mod csv;
mod error_context;
mod float;
mod future;
pub(crate) mod id;
mod int;
pub mod json;
mod map;
mod media;
mod ops;
mod ops_bitwise;
mod ops_math;
mod prompt;
mod random;
pub(crate) mod reflect;
mod resolve;
pub(crate) mod runtime_class_builder;
pub(crate) use resolve::ImplResolver;
mod root;
mod spawn;
mod stack_trace;
mod string;
mod sys;
mod time;
mod toml;
mod type_class;
pub(crate) mod type_kinds;
mod uint8array;
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
        type_args: Vec<bex_vm_types::RealizedTy>,
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

/// Returns the dispatched callee's result unchanged. Shared by the single-call
/// shims (`_compare_shim`, `string.to<T>`'s `from_string` dispatch,
/// `reflect.call_any`) whose only job is to dispatch one call and surface its
/// value.
pub(crate) struct PassThroughContinuation;

impl Continuation for PassThroughContinuation {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        NativeCallResult::Done(value)
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }
    fn apply_forwarding(&mut self, _forwarding: &HashMap<HeapPtr, HeapPtr>) {}
}

/// A typed view of an array receiver: its declared element type alongside the
/// backing slice.
///
/// Mirrors the generated class `view::` structs — the element type rides *with*
/// the receiver so a builtin that preserves it (`filter`, …) can tag its result
/// array without a side channel. Derefs to `[Value]`, so slice-only array
/// builtins take it in place of `&[Value]` with no body changes.
pub struct ArrayView<'a> {
    /// The receiver array's declared element type (`T` of `T[]`).
    pub ty: &'a bex_vm_types::RealizedTy,
    /// The receiver array's elements.
    pub data: &'a [Value],
}

impl std::ops::Deref for ArrayView<'_> {
    type Target = [Value];
    fn deref(&self) -> &[Value] {
        self.data
    }
}

/// A typed view of a map receiver: its declared key/value types alongside the
/// backing `IndexMap`.
///
/// The map analogue of [`ArrayView`] — the key and value types ride *with* the
/// receiver so a builtin that preserves them can tag its result map without a
/// side channel. Derefs to the underlying `IndexMap`, so map-only builtins take
/// it in place of `&IndexMap<BexStr, Value>` with no body changes.
pub struct MapView<'a> {
    /// The receiver map's declared key type (`K` of `map<K, V>`).
    pub key_ty: &'a bex_vm_types::RealizedTy,
    /// The receiver map's declared value type (`V` of `map<K, V>`).
    pub value_ty: &'a bex_vm_types::RealizedTy,
    /// The receiver map's entries.
    pub data: &'a indexmap::IndexMap<bex_str::BexStr, Value>,
}

impl std::ops::Deref for MapView<'_> {
    type Target = indexmap::IndexMap<bex_str::BexStr, Value>;
    fn deref(&self) -> &indexmap::IndexMap<bex_str::BexStr, Value> {
        self.data
    }
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
    clippy::get_first,
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
                let qtn = match vm.get_object(class_ptr) {
                    // `compare` methods register as globals under the class's
                    // declared FQN at emit time. An anonymous class has no
                    // declared FQN and no emitted methods, so it cannot
                    // implement `Comparable` — same as the non-instance arm.
                    Object::Class(c) => c.name.declared().cloned(),
                    _ => {
                        return Err(VmRustFnError::InternalError(
                            VmInternalError::MissingNativeFunction {
                                name: "compare dispatch: instance.class is not a Class".to_string(),
                            },
                        ));
                    }
                };
                let Some(qtn) = qtn else {
                    return Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                        message: "_compare_shim: element type does not implement Comparable"
                            .to_string(),
                    }));
                };
                format!("{}.baml.Comparable.compare", qtn.render_dotted(false))
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

    let type_args = vm.bound_method_curried_type_args(v);
    Ok(vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: fn_ptr,
        receiver: v,
        // A stdlib-dispatched `compare` on `v`'s concrete class; curry its class
        // type args (→ `Self`) from the receiver, matching a `MakeBoundMethod`.
        type_args,
    }))
}

/// For a value `v` whose runtime class implements `baml.ToString` with an
/// in-body override, resolve that `to_string` and return a
/// `BoundMethod { to_string, receiver: v }`. Returns `Ok(None)` when the
/// runtime class has no in-body override (e.g. an `implements baml.ToString {}`
/// block inheriting the structural default body, or a non-instance value) — the
/// caller then renders `v` with the structural default.
///
/// User in-body impls register as `{class_fqn}.baml.ToString.to_string`,
/// matching `make_compare_callee`'s `{class_fqn}.baml.Comparable.compare`. Used
/// by the native `baml._to_string_shim` (`root.rs`) backing `string.from`.
pub(super) fn make_to_string_callee(vm: &mut BexVm, v: Value) -> Option<HeapPtr> {
    let fn_name = to_string_override_fn_name(vm, v)?;
    let fn_ptr = vm.find_function_by_name(&fn_name)?;

    let type_args = vm.bound_method_curried_type_args(v);
    Some(vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: fn_ptr,
        receiver: v,
        // A stdlib-dispatched `to_string` override on `v`'s concrete class; curry
        // its class type args (→ `Self`) from the receiver.
        type_args,
    }))
}

/// The `{class_fqn}.baml.ToString.to_string` function name for `v`'s runtime
/// class, for the value kinds that can carry an in-body `baml.ToString` override.
/// Returns `None` for kinds that never implement `baml.ToString` (primitives,
/// arrays, maps) — `string.from` renders those structurally.
///
/// Covers `Object::Instance` (user/builtin classes) plus the two builtin
/// non-instance implementors, `type` (`baml.TypeValue`) and `uint8array`
/// (`baml.Uint8Array`); without them `string.from(v)` would diverge from
/// `v.to_string()` (e.g. a `type` rendering structurally instead of as its type
/// name, or bytes as `[1, 2]` instead of their UTF-8 text). Allocation-free
/// w.r.t. the VM heap (only builds a Rust `String`), so it is safe to call during
/// the GC-sensitive override-collection pass.
pub(super) fn to_string_override_fn_name(vm: &BexVm, v: Value) -> Option<String> {
    use bex_vm_types::ValueKind;
    let fqn = match v.kind() {
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Instance(inst) => match vm.get_object(inst.class) {
                Object::Class(c) => c.name.declared()?.render_dotted(false),
                _ => return None,
            },
            Object::Type(_) => "baml.TypeValue".to_string(),
            Object::Uint8Array(_) => "baml.Uint8Array".to_string(),
            _ => return None,
        },
        _ => return None,
    };
    Some(format!("{fqn}.baml.ToString.to_string"))
}

/// For a value `v` whose runtime class implements `baml.ToJson` with an in-body
/// override, resolve that `to_json` and return a `BoundMethod { to_json,
/// receiver: v }`. Returns `None` when the runtime class has no in-body override
/// (an `implements baml.ToJson {}` block inheriting the structural default body,
/// or a non-implementing / non-instance value) — the caller then renders `v`
/// with the structural default. The json analog of [`make_to_string_callee`].
pub(super) fn make_to_json_override_callee(vm: &mut BexVm, v: Value) -> Option<HeapPtr> {
    let fn_name = to_json_override_fn_name(vm, v)?;
    let fn_ptr = vm.find_function_by_name(&fn_name)?;
    let type_args = vm.bound_method_curried_type_args(v);
    Some(vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: fn_ptr,
        receiver: v,
        // A stdlib-dispatched `to_json` override on `v`'s concrete class; curry
        // its class type args (→ `Self`) from the receiver.
        type_args,
    }))
}

/// The `{class_fqn}.baml.ToJson.to_json` function name for `v`'s runtime class,
/// for the value kinds that can carry an in-body `baml.ToJson` override. Returns
/// `None` for kinds that never implement `baml.ToJson` (primitives, arrays, maps,
/// enums) — `baml.json.from` renders those structurally. Allocation-free w.r.t.
/// the VM heap (only builds a Rust `String`), so it is safe to call during the
/// GC-sensitive override-collection pass. The json analog of
/// [`to_string_override_fn_name`].
pub(super) fn to_json_override_fn_name(vm: &BexVm, v: Value) -> Option<String> {
    use bex_vm_types::ValueKind;
    let fqn = match v.kind() {
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Instance(inst) => match vm.get_object(inst.class) {
                Object::Class(c) => c.name.declared()?.render_dotted(false),
                _ => return None,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(format!("{fqn}.baml.ToJson.to_json"))
}

// =============================================================================
// Public module-level function wrappers
//
// `vm.rs` calls these as free functions in the `crate::native` module.
// They delegate to the generated glue methods on `VmNatives`.
// =============================================================================

/// The stdlib packages whose natives this VM implements, each paired with its
/// dispatcher. Every one dispatches through its generated root trait (see `baml_builtins2_codegen`), so no entry can drift from its
/// `.baml` declarations.
///
/// One entry per package drives both resolution and the missing-native check,
/// so adding a package is a single line here plus its `build.rs` generation.
type NativeResolver = fn(&str) -> Option<NativeFunction>;

const VM_NATIVE_PACKAGES: &[(&str, NativeResolver)] = &[
    ("baml.", PackageBamlImpl::get_native_fn),
    (
        "ai.",
        <crate::package_ai::PackageAiImpl as crate::package_ai::BamlPackageAi>::get_native_fn,
    ),
    (
        "boundary.",
        <crate::package_boundary::PackageBoundaryImpl as crate::package_boundary::BamlPackageBoundary>::get_native_fn,
    ),
];

/// Resolves native function pointers for unresolved native functions in objects.
///
/// Only functions in VM-owned native namespaces are resolved here. Functions
/// from other packages (e.g. `assert.*`, `testing.*`) are left as
/// `NativeUnresolved` so they can be wired up by future package implementations.
/// They will only fail at runtime if actually called.
pub fn attach_builtins(object: Object) -> Result<Object, VmInternalError> {
    Ok(match object {
        Object::Function(function) => {
            let kind = match function.kind {
                bex_vm_types::FunctionKind::Bytecode => bex_vm_types::FunctionKind::Bytecode,
                bex_vm_types::FunctionKind::SysOp(op) => bex_vm_types::FunctionKind::SysOp(op),
                bex_vm_types::FunctionKind::NativeUnresolved => {
                    // Only VM-owned packages resolve here; functions from other
                    // stdlib packages (assert, testing, …) stay unresolved for a
                    // future implementation to wire up.
                    let owner = VM_NATIVE_PACKAGES
                        .iter()
                        .find(|(prefix, _)| function.name.starts_with(prefix));
                    match owner.and_then(|(_, resolve)| resolve(function.name.as_str())) {
                        Some(native_function) => {
                            bex_vm_types::FunctionKind::Native(native_function as *const ())
                        }
                        // A VM-owned name with no native is a build error, not a
                        // deferral: the package's generated trait requires an
                        // implementation for every `$rust_function` it declares.
                        None if owner.is_some() => {
                            return Err(VmInternalError::MissingNativeFunction {
                                name: function.name.clone(),
                            });
                        }
                        None => bex_vm_types::FunctionKind::NativeUnresolved,
                    }
                }
                bex_vm_types::FunctionKind::Native(ptr) => bex_vm_types::FunctionKind::Native(ptr),
            };
            Object::Function(Box::new(bex_vm_types::Function {
                name: function.name,
                source_file: function.source_file,
                docstring: function.docstring,
                declared_name: function.declared_name,
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
                generic_param_bounds: function.generic_param_bounds,
                display_param_types: function.display_param_types,
                display_return_type: function.display_return_type,
                throws_type: function.throws_type,
                origin: function.origin,
                body_meta: function.body_meta,
                capture: function.capture,
                function_id: 0, // synthetic; not in the profiling function table
                runtime_package: function.runtime_package,
            }))
        }
        other => other,
    })
}
