//! Native implementations for the `baml.reflect` namespace (BEP-062, moved
//! into the `baml` package by BEP-066): `reflect.signature` and
//! `reflect.call_any` (`reflect` is the keyword shorthand for `baml.reflect`).
//!
//! Dispatch and the class constructors are generated from
//! `baml_std/baml/ns_reflect/reflect.baml` by `baml_builtins2_codegen` into
//! the `baml` package's trait hierarchy: declaring a `$rust_function` there
//! adds a required [`BamlNamespaceReflect`] method here, and each class gets a
//! `copy::reflect::` struct whose fields are checked by the compiler. Adding
//! to the namespace is therefore a single edit to `reflect.baml` plus the
//! implementation it demands. (`type.of` is a compiler intrinsic, so it never
//! reaches a native at all; `type.of_value` lives in
//! [`crate::package_baml::type_class`].)

use baml_type::{RealizedTy, Ty, TyAttr, normalize};
use bex_heap::TlabHolder;
use bex_vm_types::{
    AtomicValueSlot, HeapPtr, Object, RuntimeCompileArtifact,
    link::link_dynamic,
    types::{LocalName, MethodImpl, Package, RuntimeImplRule, RuntimePackage, TypeValue, Value},
};
use indexmap::IndexMap;

use super::{
    BamlClassReflectPackage, BamlNamespaceReflect, Continuation, NativeCallResult, PackageBamlImpl,
    PassThroughContinuation, copy,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
    vm::CallableSignature,
};

/// Element tag for the `Arg[]` / `map<string, Arg>` containers. The class
/// instances themselves are built through the generated `copy::` structs.
const ARG_FQN: &str = "baml.reflect.Arg";

impl BamlNamespaceReflect for PackageBamlImpl {
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

fn package_ptr(vm: &BexVm, value: Value) -> Result<HeapPtr, VmRustFnError> {
    let Some(wrapper_ptr) = value.as_object_ptr() else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package receiver is not an instance".to_string(),
        }
        .into());
    };
    let Object::Instance(wrapper) = vm.get_object(wrapper_ptr) else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package receiver is not an instance".to_string(),
        }
        .into());
    };
    let Some(ptr) = wrapper.load_field(0).as_object_ptr() else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package is not initialized".to_string(),
        }
        .into());
    };
    if !matches!(vm.get_object(ptr), Object::Package(_)) {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package has an invalid runtime payload".to_string(),
        }
        .into());
    }
    Ok(ptr)
}

fn local_name(path: &str) -> Option<LocalName> {
    let mut parts = path
        .split('.')
        .map(baml_type::Name::new)
        .collect::<Vec<_>>();
    let name = parts.pop()?;
    Some(LocalName {
        namespace: parts,
        name,
    })
}

fn dependency_object(vm: &BexVm, package: HeapPtr, local: &str) -> Option<HeapPtr> {
    let Object::Package(package) = vm.get_object(package) else {
        return None;
    };
    let local_name = local_name(local)?;
    package
        .classes
        .get(&local_name)
        .or_else(|| package.enums.get(&local_name))
        .or_else(|| package.interfaces.get(&local_name))
        .copied()
        .or_else(|| {
            package.runtime.as_ref().and_then(|runtime| {
                runtime
                    .functions
                    .get(&format!("user.{local}"))
                    .or_else(|| runtime.functions.get(local))
                    .copied()
            })
        })
}

fn diagnostic_value(vm: &mut BexVm, diagnostic: &bex_vm_types::RuntimeCompileDiagnostic) -> Value {
    let span = diagnostic.span.as_ref().map_or(Value::NULL, |span| {
        let file = Value::object(vm.alloc_string(span.file.as_str()));
        copy::reflect::Span {
            file,
            start: i64::try_from(span.start).expect("source offsets fit BAML int"),
            end: i64::try_from(span.end).expect("source offsets fit BAML int"),
        }
        .to_value(vm)
    });
    let code = Value::object(vm.alloc_string(diagnostic.code.as_str()));
    let message = Value::object(vm.alloc_string(diagnostic.message.as_str()));
    copy::reflect::Diagnostic {
        code,
        message,
        span,
    }
    .to_value(vm)
}

struct FinishPackage {
    wrapper: HeapPtr,
    package: HeapPtr,
}

impl Continuation for FinishPackage {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        let Object::Package(package) = vm.get_object_mut(self.package) else {
            unreachable!("finish continuation retained a Package")
        };
        package
            .runtime
            .as_mut()
            .expect("runtime package has an image")
            .initialized = true;
        NativeCallResult::Done(Value::object(self.wrapper))
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.wrapper, self.package]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.wrapper) {
            self.wrapper = ptr;
        }
        if let Some(&ptr) = forwarding.get(&self.package) {
            self.package = ptr;
        }
    }
}

impl BamlClassReflectPackage for PackageBamlImpl {
    #[allow(clippy::too_many_lines)]
    fn _finish(
        vm: &mut BexVm,
        artifact: &Value,
        packages: &IndexMap<bex_str::BexStr, Value>,
    ) -> NativeCallResult {
        let Some(artifact_inner) =
            artifact
                .as_object_ptr()
                .and_then(|ptr| match vm.get_object(ptr) {
                    Object::Instance(instance) => Some(instance.load_field(0)),
                    _ => None,
                })
        else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "reflect.Package._finish received an invalid artifact".to_string(),
            })
            .into();
        };
        let artifact = match vm.as_rust_data::<RuntimeCompileArtifact>(&artifact_inner) {
            Ok(artifact) => artifact.clone(),
            Err(error) => return error.into(),
        };
        let plan = match link_dynamic(&artifact.units) {
            Ok(plan) => plan,
            Err(error) => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: format!("runtime link failed: {error}"),
                })
                .into();
            }
        };

        let mut dependencies = IndexMap::<String, HeapPtr>::new();
        for (alias, value) in packages {
            match package_ptr(vm, *value) {
                Ok(ptr) => {
                    dependencies.insert(alias.to_string(), ptr);
                }
                Err(error) => return error.into(),
            }
        }

        let program_package = plan
            .program
            .packages
            .get(&baml_type::Name::new("user"))
            .cloned()
            .unwrap_or_default();
        let package = Package {
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            recursive_type_aliases: program_package.recursive_type_aliases.clone(),
            runtime: Some(Box::new(RuntimePackage {
                objects: Box::new([]),
                globals: Box::new([]),
                functions: IndexMap::new(),
                global_names: IndexMap::new(),
                class_types: IndexMap::new(),
                interface_blob: artifact.interface_blob,
                diagnostics: artifact.diagnostics,
                dependencies: dependencies.values().copied().collect(),
                dependency_names: dependencies.clone(),
                init: None,
                initialized: false,
            })),
        };
        let package_ptr = vm.alloc(Object::Package(Box::new(package)));

        let external_objects: std::collections::HashMap<usize, _> = plan
            .external_objects
            .iter()
            .map(|(index, symbol)| (index.raw(), symbol))
            .collect();
        let mut objects = Vec::with_capacity(plan.program.objects.len());
        for (index, object) in plan.program.objects.iter().enumerate() {
            let external = external_objects.get(&index).and_then(|symbol| {
                if matches!(symbol.kind, bex_vm_types::SymbolKind::GenericFn) {
                    return None;
                }
                vm.packages.object_by_name(&symbol.fq_name).or_else(|| {
                    let (alias, local) = symbol.fq_name.split_once('.')?;
                    dependency_object(vm, *dependencies.get(alias)?, local)
                })
            });
            if let Some(ptr) = external {
                objects.push(ptr);
                continue;
            }
            let mut object = object.clone();
            match &mut object {
                Object::Function(function) => {
                    function.runtime_package = package_ptr;
                    function.bytecode.compact = Some(function.bytecode.lower_to_compact());
                }
                Object::GenericFunction(function) => {
                    function.runtime_package = package_ptr;
                }
                _ => {}
            }
            objects.push(vm.alloc(object));
        }

        // Compile-time heap construction resolves `ConstValue::Object` into
        // stable pointers before execution. Dynamic functions need the same
        // pass after every linked object has been grafted. Keep any boxed float
        // constants in the package-owned object graph as well.
        let function_ptrs = objects
            .iter()
            .copied()
            .filter(|ptr| matches!(vm.get_object(*ptr), Object::Function(_)))
            .collect::<Vec<_>>();
        for function_ptr in function_ptrs {
            let constants = match vm.get_object(function_ptr) {
                Object::Function(function) => function.bytecode.constants.clone(),
                _ => unreachable!(),
            };
            let mut resolved = Vec::with_capacity(constants.len());
            for constant in constants {
                let value = match constant {
                    bex_vm_types::ConstValue::Type(_)
                    | bex_vm_types::ConstValue::ClassWithTypeArgs { .. } => Value::NULL,
                    bex_vm_types::ConstValue::Float(value) => {
                        let ptr = vm.alloc_float(value);
                        objects.push(ptr);
                        Value::object(ptr)
                    }
                    other => other.to_value(|index| objects[index.raw()]),
                };
                resolved.push(value);
            }
            let Object::Function(function) = vm.get_object_mut(function_ptr) else {
                unreachable!()
            };
            function.bytecode.resolved_constants = resolved;
        }

        let external_globals: std::collections::HashMap<usize, _> = plan
            .external_globals
            .iter()
            .map(|(index, symbol)| (index.raw(), symbol))
            .collect();
        let mut globals = Vec::with_capacity(plan.program.globals.len());
        for (index, value) in plan.program.globals.iter().enumerate() {
            let external = external_globals.get(&index).and_then(|symbol| {
                vm.packages
                    .global_by_name(&symbol.fq_name)
                    .map(|index| vm.globals.get(vm.proof(), index))
                    .or_else(|| {
                        let (alias, local) = symbol.fq_name.split_once('.')?;
                        let Object::Package(package) = vm.get_object(*dependencies.get(alias)?)
                        else {
                            return None;
                        };
                        let runtime = package.runtime.as_ref()?;
                        let index = runtime
                            .global_names
                            .get(&format!("user.{local}"))
                            .or_else(|| runtime.global_names.get(local))?;
                        runtime.load_global(*index)
                    })
            });
            let value = if let Some(value) = external {
                value
            } else {
                match value {
                    bex_vm_types::ConstValue::Float(value) => Value::object(vm.alloc_float(*value)),
                    other => other.to_value(|index| objects[index.raw()]),
                }
            };
            globals.push(AtomicValueSlot::new(value));
        }

        let classes = program_package
            .classes
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let enums = program_package
            .enums
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let interfaces = program_package
            .interfaces
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
            .collect::<IndexMap<_, _>>();
        let mut impl_rules = IndexMap::new();
        for (interface_index, rules) in &program_package.impl_rules {
            let interface_ptr = objects[interface_index.raw()];
            let mut pointers = Vec::with_capacity(rules.len());
            for rule in rules {
                let runtime_rule = RuntimeImplRule {
                    interface_head: objects[rule.interface_head.raw()],
                    for_ty_pattern: rule.for_ty_pattern.clone(),
                    generic_param_bounds: rule.generic_param_bounds.clone(),
                    interface_args: rule.interface_args.clone(),
                    interface_assoc: rule.interface_assoc.clone(),
                    methods: rule
                        .methods
                        .iter()
                        .map(|(name, method)| {
                            (
                                name.clone(),
                                MethodImpl {
                                    fqn: objects[method.fqn.raw()],
                                    frame: method.frame.clone(),
                                },
                            )
                        })
                        .collect(),
                    field_links: rule.field_links.clone(),
                };
                let pointer = vm.alloc(Object::ImplRule(Box::new(runtime_rule)));
                objects.push(pointer);
                pointers.push(pointer);
            }
            impl_rules.insert(interface_ptr, pointers);
        }
        let functions = plan
            .program
            .function_indices
            .iter()
            .filter(|(name, _)| name.starts_with("user."))
            .map(|(name, index)| (name.clone(), objects[*index]))
            .collect::<IndexMap<_, _>>();
        let global_names = plan
            .program
            .function_global_indices
            .iter()
            .chain(&plan.program.let_global_indices)
            .filter(|(name, _)| name.starts_with("user."))
            .map(|(name, index)| (name.clone(), *index))
            .collect::<IndexMap<_, _>>();
        let init = plan
            .program
            .package_init_order
            .iter()
            .find(|name| name.as_str() == "$init" || name.starts_with("user.$init"))
            .and_then(|name| plan.program.function_indices.get(name))
            .map(|index| objects[*index]);

        let mut class_types = IndexMap::new();
        for (name, class_ptr) in &classes {
            let Object::Class(class) = vm.get_object(*class_ptr) else {
                continue;
            };
            let ty = RealizedTy::Class(class.name.clone(), Vec::new(), TyAttr::default());
            let type_ptr = vm.alloc_type(TypeValue::runtime(
                ty,
                vm.heap.mint_runtime_id(),
                package_ptr,
            ));
            let local = name
                .namespace
                .iter()
                .map(baml_type::Name::as_str)
                .chain(std::iter::once(name.name.as_str()))
                .collect::<Vec<_>>()
                .join(".");
            class_types.insert(local, type_ptr);
        }

        let Object::Package(package) = vm.get_object_mut(package_ptr) else {
            unreachable!("package was just allocated")
        };
        package.classes = classes;
        package.enums = enums;
        package.interfaces = interfaces;
        package.impl_rules = impl_rules;
        let runtime = package.runtime.as_mut().expect("runtime package image");
        runtime.objects = objects.into_boxed_slice();
        runtime.globals = globals.into_boxed_slice();
        runtime.functions = functions;
        runtime.global_names = global_names;
        runtime.class_types = class_types;
        runtime.init = init;

        let wrapper = copy::reflect::Package {
            _inner: Value::object(package_ptr),
        }
        .to_value(vm);
        let wrapper_ptr = wrapper
            .as_object_ptr()
            .expect("Package copy helper allocates an instance");
        if let Some(init) = init {
            NativeCallResult::YieldToCall {
                callee: init,
                args: Vec::new(),
                type_args: Vec::new(),
                continuation: Box::new(FinishPackage {
                    wrapper: wrapper_ptr,
                    package: package_ptr,
                }),
            }
        } else {
            let Object::Package(package) = vm.get_object_mut(package_ptr) else {
                unreachable!()
            };
            package.runtime.as_mut().expect("runtime image").initialized = true;
            NativeCallResult::Done(wrapper)
        }
    }

    fn get_class(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let runtime = package.runtime.as_ref()?;
        let name = name.to_string();
        runtime
            .class_types
            .get(&name)
            .or_else(|| {
                runtime
                    .class_types
                    .get(name.strip_prefix("root.").unwrap_or(&name))
            })
            .copied()
            .map(Value::object)
    }

    fn classes(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        package
            .runtime
            .as_ref()
            .map_or_else(IndexMap::new, |runtime| {
                runtime
                    .class_types
                    .iter()
                    .map(|(name, ptr)| (name.as_str().into(), Value::object(*ptr)))
                    .collect()
            })
    }

    fn diagnostics(vm: &mut BexVm, package: &Value) -> Vec<Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return Vec::new();
        };
        let diagnostics = match vm.get_object(ptr) {
            Object::Package(package) => package
                .runtime
                .as_ref()
                .map(|runtime| runtime.diagnostics.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic_value(vm, diagnostic))
            .collect()
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
    let ty = Value::object(vm.alloc_static_type(ty));
    copy::reflect::Arg { name, r#type: ty }.to_value(vm)
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
    let returns = Value::object(vm.alloc_static_type(sig.ret.clone()));
    let errors = Value::object(vm.alloc_static_type(sig.throws));
    let docstring = opt_string(vm, sig.docstring.as_ref());
    let name = opt_string(vm, sig.name.as_ref());
    Ok(copy::reflect::Signature {
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
    let expected = Value::object(vm.alloc_static_type(expected));
    let got = Value::object(vm.alloc_static_type(got));
    let err = copy::reflect::InvalidArgumentError {
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
