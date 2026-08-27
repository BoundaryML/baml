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
pub(crate) mod resolve;
pub(crate) use resolve::ImplResolver;
pub(crate) mod root;
mod spawn;
mod stack_trace;
mod string;
mod sys;
mod time;
mod toml;
mod uint8array;
mod unknown_error;
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
/// shims (`_compare_shim` and `string.to<T>`'s `from_string` dispatch) whose
/// only job is to dispatch one call and surface its value.
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
    let resolved =
        shim_rule_method(vm, v, "Comparable", "compare").map_err(VmRustFnError::InternalError)?;
    let Some(resolved) = resolved else {
        return Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument {
            message: "_compare_shim: element type does not implement Comparable".to_string(),
        }));
    };
    Ok(rule_bound_method(vm, v, resolved))
}

/// A rule-resolved shim callee: one method entry of the receiver's impl rule,
/// with its realized frame.
pub(super) struct ShimRuleMethod {
    /// The resolved body: the impl's provided method, or the interface's
    /// default.
    callee: HeapPtr,
    /// The callee's realized owner frame (`[impl bindings..]` for a
    /// provided method, `[Self, iface args..]` for an adopted default).
    type_args: Vec<bex_vm_types::RealizedTy>,
    /// `true` when `callee` IS the interface's default body — the shims'
    /// no-provided-method case (their structural fallback renders exactly
    /// what the default body delegates to).
    pub(super) is_default: bool,
}

/// The rule-resolved `method` entry behind a value-position stdlib shim: `v`'s
/// impl of the root-package interface `iface_name`, read off the impl-rule
/// tables exactly as `dispatch_op` (ops.rs) reads operators — never by
/// constructing a mangled global name, so blanket and out-of-body impls and
/// runtime-declared classes all resolve. Pure resolution over `&BexVm` with no
/// VM-heap allocation, so it is safe in GC-sensitive collection passes.
///
/// `Ok(None)` = no concrete receiver type or no applicable rule — the shims'
/// structural-fallback case. A frame that fails to realize is an internal
/// error, never a silent fallback.
pub(super) fn shim_rule_method(
    vm: &BexVm,
    v: Value,
    iface_name: &str,
    method: &str,
) -> Result<Option<ShimRuleMethod>, VmInternalError> {
    let qtn = baml_type::TypeName::new(
        baml_type::Name::new("baml"),
        Vec::new(),
        baml_type::Name::new(iface_name),
    );
    // A stdlib FQN constant is one of the places a name legitimately becomes
    // a head; it resolves once, off the declaration.
    let Some(iface_head) = vm.declaration_head(&qtn) else {
        return Ok(None);
    };
    let Some(self_ty) = vm.value_concrete_ty(v) else {
        return Ok(None);
    };
    let resolver = resolve::ImplResolver::for_value(vm, v);
    let Some((rule, bound_args)) =
        resolver.resolve_implements_rule(&self_ty.into(), iface_head, &[])
    else {
        return Ok(None);
    };
    let Some(resolved) = resolver.rule_method_impl(&rule, method) else {
        return Ok(None);
    };
    let type_args = resolver.realize_frame(&resolved.method.frame, &bound_args)?;
    Ok(Some(ShimRuleMethod {
        callee: resolved.method.fqn,
        type_args,
        is_default: resolved.is_default,
    }))
}

/// Bind a [`shim_rule_method`] resolution to its receiver: the curried frame
/// is the rule's realized method frame — exact, not the receiver-derived
/// approximation `bound_method_curried_type_args` gives a `MakeBoundMethod`.
fn rule_bound_method(vm: &mut BexVm, v: Value, resolved: ShimRuleMethod) -> HeapPtr {
    vm.alloc_bound_method(bex_vm_types::BoundMethod {
        function: resolved.callee,
        receiver: v,
        type_args: resolved.type_args.into_boxed_slice(),
    })
}

/// For a value `v` whose impl of `baml.ToString` provides its own
/// `to_string`, resolve it through the impl rules and return a
/// `BoundMethod { to_string, receiver: v }`. `Ok(None)` when `v`'s type has no
/// rule or its rule adopts the structural default body — the caller then
/// renders `v` with the structural default. Used by the native
/// `baml._to_string_shim` (`root.rs`) backing `string.from`.
pub(super) fn make_to_string_callee(
    vm: &mut BexVm,
    v: Value,
) -> Result<Option<HeapPtr>, VmInternalError> {
    match shim_rule_method(vm, v, "ToString", "to_string")? {
        Some(resolved) if !resolved.is_default => Ok(Some(rule_bound_method(vm, v, resolved))),
        _ => Ok(None),
    }
}

/// For a value `v` whose impl of `baml.ToJson` provides its own `to_json`,
/// resolve it through the impl rules and return a
/// `BoundMethod { to_json, receiver: v }`. `Ok(None)` when `v`'s type has no
/// rule or its rule adopts the structural default body — the caller then
/// renders `v` with the structural default. The json analog of
/// [`make_to_string_callee`].
pub(super) fn make_to_json_callee(
    vm: &mut BexVm,
    v: Value,
) -> Result<Option<HeapPtr>, VmInternalError> {
    match shim_rule_method(vm, v, "ToJson", "to_json")? {
        Some(resolved) if !resolved.is_default => Ok(Some(rule_bound_method(vm, v, resolved))),
        _ => Ok(None),
    }
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
    (
        "reflect.",
        <crate::package_reflect::PackageReflectImpl as crate::package_reflect::BamlPackageReflect>::get_native_fn,
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
                    // Dispatch through the dedicated `native_key` (minted by
                    // emit for `$rust_function` bodies), never the display
                    // name. Only VM-owned packages resolve here; functions
                    // from other stdlib packages (assert, testing, …) stay
                    // unresolved for a future implementation to wire up.
                    let owner = function.native_key.as_deref().and_then(|key| {
                        VM_NATIVE_PACKAGES
                            .iter()
                            .find(|(prefix, _)| key.starts_with(prefix))
                            .map(|(_, resolve)| (key, resolve))
                    });
                    match owner {
                        Some((key, resolve)) => match resolve(key) {
                            Some(native_function) => {
                                bex_vm_types::FunctionKind::Native(native_function as *const ())
                            }
                            // A VM-owned key with no native is a build error,
                            // not a deferral: the package's generated trait
                            // requires an implementation for every
                            // `$rust_function` it declares.
                            None => {
                                return Err(VmInternalError::MissingNativeFunction {
                                    name: function.name.clone(),
                                });
                            }
                        },
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
                is_interface_body: function.is_interface_body,
                native_key: function.native_key,
                body_meta: function.body_meta,
                capture: function.capture,
                function_id: 0, // synthetic; not in the profiling function table
                runtime_package: function.runtime_package,
            }))
        }
        other => other,
    })
}
