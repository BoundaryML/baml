//! Native implementations for the `reflect` stdlib package (BEP-062):
//! `reflect.signature` and `reflect.call_any`.
//!
//! Dispatch and the class constructors are generated from `reflect.baml` by
//! `baml_builtins2_codegen`, exactly as for [`crate::package_baml`]: declaring
//! a `$rust_function` adds a required trait method here, and each class gets a
//! `copy::` struct whose fields are checked by the compiler. Adding to the
//! package is therefore a single edit to `reflect.baml` plus the implementation
//! it demands. (`reflect.type_of` is a compiler intrinsic, so it never reaches
//! a native at all.)

use baml_type::{RealizedTy, Ty, TyAttr, normalize};
use bex_heap::TlabHolder;
// `Instance`/`Type`/the read guards are referenced by the generated module.
use bex_vm_types::{
    ArrayReadGuard, MapReadGuard,
    types::{Instance, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
    package_baml::{
        NativeCallResult, NativeFunction, NativeFunctionResult, PassThroughContinuation,
    },
    vm::CallableSignature,
};

/// Element tag for the `Arg[]` / `map<string, Arg>` containers. The class
/// instances themselves are built through the generated `copy::` structs.
const ARG_FQN: &str = "reflect.Arg";

// The `BamlPackageReflect` trait and its `get_native_fn` dispatcher, generated
// from `reflect.baml` exactly like the `baml` package's (see
// `crate::package_baml`). Declaring a `$rust_function` there adds a required
// trait method here, so the two can never drift.
#[allow(
    unused_variables,
    unsafe_code,
    non_camel_case_types,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::get_first,
    clippy::iter_not_returning_iterator,
    clippy::needless_lifetimes,
    clippy::redundant_closure_call,
    clippy::new_ret_no_self,
    clippy::too_many_arguments,
    non_snake_case
)]
mod generated {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/reflectfunctions_generated.rs"));
}
pub use generated::*;

/// The VM's `reflect` native implementations.
pub struct PackageReflectImpl;

impl BamlPackageReflect for PackageReflectImpl {
    fn signature(vm: &mut BexVm, f: &Value) -> Result<Value, VmRustFnError> {
        signature_impl(vm, *f)
    }

    fn call_any(
        vm: &mut BexVm,
        f: &Value,
        args: &IndexMap<bex_str::BexStr, Value>,
    ) -> NativeCallResult {
        call_any_impl(vm, *f, args)
    }
}

fn ty_never() -> RealizedTy {
    RealizedTy::Never {
        attr: TyAttr::default(),
    }
}

/// The two natives' parameters are statically `baml.AnyFunction`, so a
/// non-callable here means the coercion rule and the runtime disagree — an
/// internal invariant break, not a user error.
fn non_callable_error(what: &str) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument {
        message: format!("{what} expects a function value"),
    })
}

/// The `reflect.Arg` class type, for array/map element tags.
fn ty_arg() -> RealizedTy {
    RealizedTy::Class(
        baml_type::QualifiedTypeName::from_dotted_path(ARG_FQN),
        vec![],
        TyAttr::default(),
    )
}

/// Build one `reflect.Arg`. A nameless positional (a host callable from a
/// language without parameter-name introspection) gets the `$argN`
/// placeholder for its position: `$` is unwritable in user identifiers, so a
/// placeholder can never collide with a declared parameter or a
/// named-argument key.
fn alloc_arg(
    vm: &mut BexVm,
    name: Option<&baml_type::Name>,
    position: usize,
    ty: RealizedTy,
) -> Value {
    let name = match name {
        Some(n) => Value::object(vm.alloc_string(n.as_str())),
        None => Value::object(vm.alloc_string(format!("$arg{position}"))),
    };
    let ty = Value::object(vm.tlab.alloc_type(ty));
    copy::Arg { name, r#type: ty }.to_value(vm)
}

/// A `string?` field: the string, or null.
fn opt_string(vm: &mut BexVm, value: Option<&String>) -> Value {
    match value {
        Some(v) => Value::object(vm.alloc_string(v.as_str())),
        None => Value::NULL,
    }
}

/// `reflect.signature(f) -> reflect.Signature`.
fn signature_impl(vm: &mut BexVm, f_val: Value) -> Result<Value, VmRustFnError> {
    use baml_type::FunctionParamMode;
    let Some(sig) = vm.callable_signature(f_val) else {
        return Err(non_callable_error("reflect.signature"));
    };
    let mut positional = Vec::new();
    let mut opts: IndexMap<bex_str::BexStr, Value> = IndexMap::new();
    for param in &sig.params {
        match param.mode {
            FunctionParamMode::Required => {
                let position = positional.len();
                let arg = alloc_arg(vm, param.name.as_ref(), position, param.ty.clone());
                positional.push(arg);
            }
            FunctionParamMode::Optional => {
                // An optional parameter always has a source name; a nameless
                // one is unaddressable by callers (there is nothing to pass
                // it by), so it is simply absent from `opts`. Placeholders
                // are for positionals only and never enter by-name matching.
                if let Some(name) = &param.name {
                    let arg = alloc_arg(vm, Some(name), positional.len(), param.ty.clone());
                    opts.insert(bex_str::BexStr::from(name.as_str()), arg);
                }
            }
        }
    }
    let args = Value::object(vm.tlab.alloc_array(ty_arg(), positional));
    let opts = Value::object(vm.tlab.alloc_map(RealizedTy::string(), ty_arg(), opts));
    let returns = Value::object(vm.tlab.alloc_type(sig.ret.clone()));
    let errors = Value::object(vm.tlab.alloc_type(sig.throws));
    let docstring = opt_string(vm, sig.docstring.as_ref());
    let name = opt_string(vm, sig.name.as_ref());
    Ok(copy::Signature {
        name,
        args,
        opts,
        returns,
        errors,
        docstring,
    }
    .to_value(vm))
}

/// Throw `reflect.InvalidArgumentError { argument, expected, got }`.
fn raise_invalid_argument(
    vm: &mut BexVm,
    argument: &str,
    expected: RealizedTy,
    got: RealizedTy,
) -> NativeCallResult {
    let argument = Value::object(vm.alloc_string(argument));
    let expected = Value::object(vm.tlab.alloc_type(expected));
    let got = Value::object(vm.tlab.alloc_type(got));
    let err = copy::InvalidArgumentError {
        argument,
        expected,
        got,
    }
    .to_value(vm);
    NativeCallResult::Error(VmRustFnError::Thrown(err))
}

/// The callee's whole function type, for arity / unknown-name mismatches.
fn callee_fn_ty(sig: &CallableSignature) -> RealizedTy {
    RealizedTy::Function {
        params: sig.params.clone(),
        ret: Box::new(sig.ret.clone()),
        throws: Box::new(sig.throws.clone()),
        attr: TyAttr::default(),
    }
}

/// A value's reconstructed type, `unknown` when it has none (a bound method, an
/// opaque handle). A future reconstructs to the `Future<T, E>` it was spawned at.
fn value_realized_ty(vm: &BexVm, value: Value) -> RealizedTy {
    vm.value_concrete_ty(value)
        .map_or_else(RealizedTy::unknown, RealizedTy::from)
}

/// Whether `value` fits the parameter type `expected`, by the canonical
/// algebra over the runtime context. Fails OPEN when the value's type cannot
/// be reconstructed — an opaque native handle (see `value_concrete_ty`) has no
/// BAML type to compare against, and refusing what we cannot check would
/// reject working calls; the callee remains dynamically safe either way
/// (values stay tagged).
fn value_fits(vm: &BexVm, value: Value, expected: &RealizedTy) -> bool {
    let Some(actual) = vm.value_concrete_ty(value) else {
        return true;
    };
    // No convention patching is needed on the way in: a reconstructed
    // signature spells "cannot throw" as `never`, exactly as the static
    // algebra does.
    let actual: Ty = actual.into();
    let expected: Ty = expected.clone().into();
    // The VM itself is the runtime `TypeContext`.
    normalize::is_subtype(&actual, &expected, vm)
}

/// The boundary decode at the reflection call boundary (B-1174): when a
/// provided value does not fit a parameter's type as-is, a value-level,
/// lossless conversion may construct a FRESH value that does.
///
/// Dynamic argument maps arrive from sources that follow JSON Schema — an
/// LLM emits `{"budget": 150}` for a `float` parameter, and a decoded JSON
/// array carries the element tag `json` (`baml.json.json[]`) regardless of
/// what a `string[]` parameter expects. The language itself keeps no
/// implicit coercion and arrays stay invariant; this is not a subtyping
/// rule. It applies only where `call_any` validates `unknown` values
/// against the callee's signature, and every conversion allocates a new
/// value, so nothing is aliased and nothing existing is retyped.
///
/// Two conversions exist, applied recursively through arrays:
/// - an exactly representable integer widens to `float`
///   ([`widen_int_for_float`]);
/// - an array whose every element fits (or decodes into) the parameter's
///   element type rebuilds as a fresh array of that element type, which
///   admits `json[]` into `string[]`, `[150, 200]` into `float[]`, and
///   nested compositions.
///
/// Returns `None` when no conversion applies; the caller reports the
/// ordinary `InvalidArgumentError`.
fn decode_for_param(vm: &mut BexVm, value: Value, expected: &RealizedTy) -> Option<Value> {
    if let Some(widened) = widen_int_for_float(vm, value, expected) {
        return Some(widened);
    }
    if let RealizedTy::List(elem_ty, _) = expected.strip_null() {
        // Copy the elements out first: the read guard borrows the VM, and
        // the per-element decode needs it mutably.
        let elems: Vec<Value> = vm.as_array(&value).ok()?.to_vec();
        let mut out = Vec::with_capacity(elems.len());
        for e in elems {
            let decoded = if value_fits(vm, e, &elem_ty) {
                e
            } else {
                decode_for_param(vm, e, &elem_ty)?
            };
            out.push(decoded);
        }
        return Some(Value::object(vm.tlab.alloc_array(*elem_ty, out)));
    }
    None
}

/// An integer value supplied for a `float` (or `float?`) parameter widens
/// to float — JSON Schema's `number` admits integral literals. Returns
/// `None` when the widening does not apply — non-int values, non-float
/// parameters — and nothing else converts (no float→int truncation, no
/// string parsing).
fn widen_int_for_float(vm: &mut BexVm, value: Value, expected: &RealizedTy) -> Option<Value> {
    let i = value.as_int()?;
    // `strip_null` reduces `float?` (the nullable union) to its non-null
    // member, so plain `float` and `float?` both widen; a wider union such
    // as `float | string` stays an error, as does everything else.
    match expected.strip_null() {
        RealizedTy::Float { .. } => {
            // Widen only when the conversion is exact. An f64 represents
            // every integer up to 2^53; beyond that, silent precision loss
            // would corrupt the argument, so the value stays an
            // InvalidArgumentError like any other mismatch.
            #[allow(clippy::cast_precision_loss)]
            let f = i as f64;
            #[allow(clippy::cast_possible_truncation)]
            if f as i64 == i {
                Some(Value::object(vm.alloc_float(f)))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `reflect.call_any<R, E>(f, args) -> R throws E | InvalidArgumentError`.
///
/// Every argument is keyed by parameter name; a nameless positional is
/// addressed by the same `$argN` placeholder `reflect.signature` reports, so
/// the signature's keys are exactly the accepted keys. Checks the map
/// against `f`'s runtime signature (a missing required parameter, a key
/// naming no parameter, or an ill-typed value throws `InvalidArgumentError`),
/// then dispatches through the CPS trampoline. Absent optionals are passed
/// as `OMITTED_ARG`, so a bytecode callee's own default prologue fires; the
/// callee's throw unwinds transparently past the native frame to the caller,
/// which is exactly the declared `throws E` channel.
fn call_any_impl(
    vm: &mut BexVm,
    f_val: Value,
    provided: &IndexMap<bex_str::BexStr, Value>,
) -> NativeCallResult {
    use baml_type::FunctionParamMode;
    let Some(f_ptr) = f_val.as_object_ptr() else {
        return non_callable_error("reflect.call_any").into();
    };
    let Some(sig) = vm.callable_signature(f_val) else {
        return non_callable_error("reflect.call_any").into();
    };

    // Walk the parameters in declaration order, resolving each from the map
    // by its addressable name and assembling the callee's frame as we go.
    // Absent optionals become `OMITTED_ARG`: a bytecode callee's default
    // prologue replaces them; a native callee's glue reads them as "not
    // supplied". A nameless optional is unaddressable and always omitted.
    let mut final_args = Vec::with_capacity(sig.params.len());
    let mut addressable: Vec<String> = Vec::with_capacity(sig.params.len());
    let mut matched = 0usize;
    let mut positional_idx = 0usize;
    for param in &sig.params {
        let key = match (&param.name, param.mode) {
            (Some(name), _) => Some(name.as_str().to_string()),
            (None, FunctionParamMode::Required) => Some(format!("$arg{positional_idx}")),
            (None, FunctionParamMode::Optional) => None,
        };
        if param.mode == FunctionParamMode::Required {
            positional_idx += 1;
        }
        let value = key.as_deref().and_then(|k| provided.get(k).copied());
        if let Some(k) = key.clone() {
            addressable.push(k);
        }
        match (param.mode, value) {
            (_, Some(value)) => {
                let value = if value_fits(vm, value, &param.ty) {
                    value
                } else if let Some(decoded) = decode_for_param(vm, value, &param.ty) {
                    decoded
                } else {
                    let expected = param.ty.clone();
                    let got = value_realized_ty(vm, value);
                    return raise_invalid_argument(vm, key.as_deref().unwrap_or(""), expected, got);
                };
                matched += 1;
                final_args.push(value);
            }
            (FunctionParamMode::Required, None) => {
                // Missing required parameter: its type against `never` (no
                // value was supplied at all).
                return raise_invalid_argument(
                    vm,
                    key.as_deref().unwrap_or(""),
                    param.ty.clone(),
                    ty_never(),
                );
            }
            (FunctionParamMode::Optional, None) => final_args.push(Value::OMITTED_ARG),
        }
    }

    // Every provided key must have matched a parameter (names are unique, so
    // the counts agree exactly when no key was extraneous). Name the first
    // key that addresses no parameter.
    if matched != provided.len() {
        let unknown = provided
            .keys()
            .find(|k| !addressable.iter().any(|a| a == k.as_str()))
            .map(|k| k.as_str().to_string())
            .unwrap_or_default();
        let got = provided
            .get(unknown.as_str())
            .copied()
            .map_or_else(RealizedTy::unknown, |v| value_realized_ty(vm, v));
        let expected = callee_fn_ty(&sig);
        return raise_invalid_argument(vm, &unknown, expected, got);
    }

    NativeCallResult::YieldToCall {
        callee: f_ptr,
        args: final_args,
        type_args: vec![],
        continuation: Box::new(PassThroughContinuation),
    }
}
