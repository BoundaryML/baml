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
    type_context::RuntimeTypeContext,
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
    NativeCallResult::Done(Value::object(vm.alloc_instance(
        class_ptr,
        vec![args_val, opts_val, returns_val, errors_val, docstring_val],
    )))
}

/// Throw `reflect.InvalidArgumentError { expected, got }`.
fn raise_invalid_argument(
    vm: &mut BexVm,
    expected: RealizedTy,
    got: RealizedTy,
) -> NativeCallResult {
    let Some(class_ptr) = vm.lookup_type_by_fqn(INVALID_ARGUMENT_ERROR_FQN) else {
        return VmInternalError::MissingNativeFunction {
            name: INVALID_ARGUMENT_ERROR_FQN.to_string(),
        }
        .into();
    };
    let expected_val = Value::object(vm.tlab.alloc_type(expected));
    let got_val = Value::object(vm.tlab.alloc_type(got));
    NativeCallResult::Error(VmRustFnError::Thrown(Value::object(
        vm.alloc_instance(class_ptr, vec![expected_val, got_val]),
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

/// The call's shape as a function type: the provided positionals' reconstructed
/// types plus each provided named argument, with `unknown` return/throws.
fn call_shape_ty(
    vm: &BexVm,
    provided: &[Value],
    opts: &IndexMap<bex_str::BexStr, Value>,
) -> RealizedTy {
    use baml_type::{FunctionParamMode, RealizedFunctionParamTy};
    let mut params: Vec<RealizedFunctionParamTy> = provided
        .iter()
        .map(|&v| RealizedFunctionParamTy {
            name: None,
            ty: value_realized_ty(vm, v),
            mode: FunctionParamMode::Required,
        })
        .collect();
    params.extend(opts.iter().map(|(k, &v)| RealizedFunctionParamTy {
        name: Some(baml_type::Name::new(k.as_str())),
        ty: value_realized_ty(vm, v),
        mode: FunctionParamMode::Optional,
    }));
    RealizedTy::Function {
        params,
        ret: Box::new(RealizedTy::unknown()),
        throws: Box::new(RealizedTy::unknown()),
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
    normalize::is_subtype(&actual, &expected, &RuntimeTypeContext::new(vm))
}

/// `reflect.call_any<R, E>(f, args, opts = {}) -> R throws E | InvalidArgumentError`.
///
/// Checks the runtime-assembled arguments against `f`'s runtime signature,
/// then dispatches through the CPS trampoline. Absent optionals are passed as
/// `OMITTED_ARG`, so a bytecode callee's own default prologue fires; the
/// callee's throw unwinds transparently past the native frame to the caller,
/// which is exactly the declared `throws E` channel.
fn call_any(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
    use baml_type::FunctionParamMode;
    let (Some(&f_val), Some(&pos_val)) = (args.first(), args.get(1)) else {
        return VmInternalError::MissingNativeFunction {
            name: "reflect.call_any".to_string(),
        }
        .into();
    };
    let opts_val = args.get(2).copied().unwrap_or(Value::OMITTED_ARG);

    let Some(f_ptr) = f_val.as_object_ptr() else {
        return non_callable_error("reflect.call_any");
    };
    let Some(sig) = vm.callable_signature(f_val) else {
        return non_callable_error("reflect.call_any");
    };

    let provided: Vec<Value> = match pos_val.as_object_ptr().map(|p| vm.get_object(p)) {
        Some(Object::Array(arr)) => arr.to_vec(),
        _ => {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "reflect.call_any expects an array of positional arguments".to_string(),
            })
            .into();
        }
    };
    let opts: IndexMap<bex_str::BexStr, Value> = if opts_val.is_omitted() {
        IndexMap::new()
    } else {
        match opts_val.as_object_ptr().map(|p| vm.get_object(p)) {
            Some(Object::Map(map)) => map.to_index_map(),
            _ => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "reflect.call_any expects a map of named arguments".to_string(),
                })
                .into();
            }
        }
    };

    let required: Vec<&baml_type::RealizedFunctionParamTy> = sig
        .params
        .iter()
        .filter(|p| p.mode == FunctionParamMode::Required)
        .collect();

    // Positional arity, then per-value fit.
    if provided.len() != required.len() {
        let expected = callee_fn_ty(&sig);
        let got = call_shape_ty(vm, &provided, &opts);
        return raise_invalid_argument(vm, expected, got);
    }
    for (&value, param) in provided.iter().zip(&required) {
        if !value_fits(vm, value, &param.ty) {
            let expected = param.ty.clone();
            let got = value_realized_ty(vm, value);
            return raise_invalid_argument(vm, expected, got);
        }
    }

    // Named arguments: every key must name a declared optional, and fit it.
    for (key, &value) in &opts {
        let Some(param) = sig.params.iter().find(|p| {
            p.mode == FunctionParamMode::Optional
                && p.name.as_ref().is_some_and(|n| n.as_str() == key.as_str())
        }) else {
            let expected = callee_fn_ty(&sig);
            let got = call_shape_ty(vm, &provided, &opts);
            return raise_invalid_argument(vm, expected, got);
        };
        if !value_fits(vm, value, &param.ty) {
            let expected = param.ty.clone();
            let got = value_realized_ty(vm, value);
            return raise_invalid_argument(vm, expected, got);
        }
    }

    // Assemble the callee's frame in parameter order. Absent optionals become
    // `OMITTED_ARG`: a bytecode callee's default prologue replaces them; a
    // native callee's glue reads them as "not supplied".
    let mut final_args = Vec::with_capacity(sig.params.len());
    let mut positional = provided.into_iter();
    for param in &sig.params {
        match param.mode {
            // The arity check above guarantees one provided value per
            // required param; the fallback is unreachable.
            FunctionParamMode::Required => {
                final_args.push(positional.next().unwrap_or(Value::OMITTED_ARG));
            }
            FunctionParamMode::Optional => final_args.push(
                param
                    .name
                    .as_ref()
                    .and_then(|n| opts.get(n.as_str()).copied())
                    .unwrap_or(Value::OMITTED_ARG),
            ),
        }
    }

    NativeCallResult::YieldToCall {
        callee: f_ptr,
        args: final_args,
        type_args: vec![],
        continuation: Box::new(PassThroughContinuation),
    }
}
