//! Native implementations for the `reflect` stdlib package (BEP-062):
//! `reflect.signature` and `reflect.call_any`. Hand-wired like
//! [`crate::package_boundary`] (the package is small enough not to warrant the
//! generated trait machinery); `reflect.type_of` is a compiler intrinsic and
//! never dispatches here.

use baml_type::{RealizedTy, Ty, TyAttr, normalize};
use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr,
    types::{Object, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
    package_baml::{NativeCallResult, NativeFunction, PassThroughContinuation},
    vm::CallableSignature,
};

const SIGNATURE_FQN: &str = "reflect.Signature";
const ARG_FQN: &str = "reflect.Arg";
const INVALID_ARGUMENT_ERROR_FQN: &str = "reflect.InvalidArgumentError";

pub fn get_native_fn(path: &str) -> Option<NativeFunction> {
    match path.strip_prefix("reflect.")? {
        "signature" => Some(signature),
        "call_any" => Some(call_any),
        _ => None,
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
fn non_callable_error(what: &str) -> NativeCallResult {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument {
        message: format!("{what} expects a function value"),
    })
    .into()
}

/// The `reflect.Arg` class type, for array/map element tags.
fn ty_arg() -> RealizedTy {
    RealizedTy::Class(
        baml_type::QualifiedTypeName::from_dotted_path(ARG_FQN),
        vec![],
        TyAttr::default(),
    )
}

/// Allocate one `reflect.Arg { name, type }` instance. A nameless positional
/// (a host callable from a language without parameter-name introspection)
/// gets the `$argN` placeholder for its position: `$` is unwritable in user
/// identifiers, so the placeholder cannot collide with any declared
/// parameter or named-argument key.
fn alloc_arg(
    vm: &mut BexVm,
    arg_class: HeapPtr,
    name: Option<&baml_type::Name>,
    position: usize,
    ty: RealizedTy,
) -> Value {
    let name_val = match name {
        Some(n) => Value::object(vm.alloc_string(n.as_str())),
        None => Value::object(vm.alloc_string(format!("$arg{position}"))),
    };
    let ty_val = Value::object(vm.tlab.alloc_type(ty));
    Value::object(vm.alloc_instance(arg_class, vec![name_val, ty_val]))
}

/// `reflect.signature(f) -> reflect.Signature`.
fn signature(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
    use baml_type::FunctionParamMode;
    let Some(&f_val) = args.first() else {
        return VmInternalError::MissingNativeFunction {
            name: "reflect.signature".to_string(),
        }
        .into();
    };
    let Some(sig) = vm.callable_signature(f_val) else {
        return non_callable_error("reflect.signature");
    };
    let (Some(class_ptr), Some(arg_class)) = (
        vm.lookup_type_by_fqn(SIGNATURE_FQN),
        vm.lookup_type_by_fqn(ARG_FQN),
    ) else {
        return VmInternalError::MissingNativeFunction {
            name: SIGNATURE_FQN.to_string(),
        }
        .into();
    };
    let mut positional = Vec::new();
    let mut opts: IndexMap<bex_str::BexStr, Value> = IndexMap::new();
    for param in &sig.params {
        match param.mode {
            FunctionParamMode::Required => {
                let position = positional.len();
                let arg_val = alloc_arg(
                    vm,
                    arg_class,
                    param.name.as_ref(),
                    position,
                    param.ty.clone(),
                );
                positional.push(arg_val);
            }
            FunctionParamMode::Optional => {
                // An optional parameter always has a source name; a nameless
                // one is unaddressable by callers (there is nothing to pass
                // it by), so it is simply absent from `opts`. Placeholders
                // are for positionals only and never enter by-name matching.
                if let Some(name) = &param.name {
                    let arg_val = alloc_arg(
                        vm,
                        arg_class,
                        Some(name),
                        positional.len(),
                        param.ty.clone(),
                    );
                    opts.insert(bex_str::BexStr::from(name.as_str()), arg_val);
                }
            }
        }
    }
    let args_val = Value::object(vm.tlab.alloc_array(ty_arg(), positional));
    let opts_val = Value::object(vm.tlab.alloc_map(RealizedTy::string(), ty_arg(), opts));
    let returns_val = Value::object(vm.tlab.alloc_type(sig.ret.clone()));
    let errors_val = Value::object(vm.tlab.alloc_type(sig.throws.unwrap_or_else(ty_never)));
    let docstring_val = match &sig.docstring {
        Some(doc) => Value::object(vm.alloc_string(doc.as_str())),
        None => Value::NULL,
    };
    let name_val = match &sig.name {
        Some(name) => Value::object(vm.alloc_string(name.as_str())),
        None => Value::NULL,
    };
    NativeCallResult::Done(Value::object(vm.alloc_instance(
        class_ptr,
        vec![
            name_val,
            args_val,
            opts_val,
            returns_val,
            errors_val,
            docstring_val,
        ],
    )))
}

/// Throw `reflect.InvalidArgumentError { argument, expected, got }`.
fn raise_invalid_argument(
    vm: &mut BexVm,
    argument: &str,
    expected: RealizedTy,
    got: RealizedTy,
) -> NativeCallResult {
    let Some(class_ptr) = vm.lookup_type_by_fqn(INVALID_ARGUMENT_ERROR_FQN) else {
        return VmInternalError::MissingNativeFunction {
            name: INVALID_ARGUMENT_ERROR_FQN.to_string(),
        }
        .into();
    };
    let argument_val = Value::object(vm.alloc_string(argument));
    let expected_val = Value::object(vm.tlab.alloc_type(expected));
    let got_val = Value::object(vm.tlab.alloc_type(got));
    NativeCallResult::Error(VmRustFnError::Thrown(Value::object(
        vm.alloc_instance(class_ptr, vec![argument_val, expected_val, got_val]),
    )))
}

/// The callee's whole function type, for arity / unknown-name mismatches.
fn callee_fn_ty(sig: &CallableSignature) -> RealizedTy {
    RealizedTy::Function {
        params: sig.params.clone(),
        ret: Box::new(sig.ret.clone()),
        throws: Box::new(sig.throws.clone().unwrap_or_else(ty_never)),
        attr: TyAttr::default(),
    }
}

/// A value's reconstructed type, `unknown` when it has none (a nested function
/// value, a future, an opaque handle).
fn value_realized_ty(vm: &BexVm, value: Value) -> RealizedTy {
    vm.value_concrete_ty(value)
        .map_or_else(RealizedTy::unknown, RealizedTy::from)
}

/// Whether `value` fits the parameter type `expected`, by the canonical
/// algebra over the runtime context. Fails OPEN when the value's type cannot
/// be reconstructed (futures, opaque handles, bound methods): the stored
/// signature may itself carry erased `unknown` slots, so refusing what we
/// cannot check would reject working calls; the callee remains dynamically
/// safe either way (values stay tagged).
fn value_fits(vm: &BexVm, value: Value, expected: &RealizedTy) -> bool {
    let Some(actual) = vm.value_concrete_ty(value) else {
        return true;
    };
    let mut actual: Ty = actual.into();
    // A reconstructed closure signature spells "cannot throw" as `void`
    // (`function_object_ty`); the static algebra spells it `never`. Patch the
    // top level so passing a non-throwing function where one is expected
    // works; nested occurrences keep the known convention gap
    // (`FIXME(function-type-matching)` in emit).
    if let Ty::Function { throws, .. } = &mut actual
        && matches!(**throws, Ty::Void { .. })
    {
        **throws = Ty::Never {
            attr: TyAttr::default(),
        };
    }
    let expected: Ty = expected.clone().into();
    // The VM itself is the runtime `TypeContext`.
    normalize::is_subtype(&actual, &expected, vm)
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
fn call_any(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
    use baml_type::FunctionParamMode;
    let (Some(&f_val), Some(&args_val)) = (args.first(), args.get(1)) else {
        return VmInternalError::MissingNativeFunction {
            name: "reflect.call_any".to_string(),
        }
        .into();
    };

    let Some(f_ptr) = f_val.as_object_ptr() else {
        return non_callable_error("reflect.call_any");
    };
    let Some(sig) = vm.callable_signature(f_val) else {
        return non_callable_error("reflect.call_any");
    };

    let provided: IndexMap<bex_str::BexStr, Value> =
        match args_val.as_object_ptr().map(|p| vm.get_object(p)) {
            Some(Object::Map(map)) => map.to_index_map(),
            _ => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "reflect.call_any expects a map of arguments".to_string(),
                })
                .into();
            }
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
                if !value_fits(vm, value, &param.ty) {
                    let expected = param.ty.clone();
                    let got = value_realized_ty(vm, value);
                    return raise_invalid_argument(vm, key.as_deref().unwrap_or(""), expected, got);
                }
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
