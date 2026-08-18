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

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicBool},
};

use baml_compiler_diagnostics::{
    DiagnosticId, DiagnosticPhase,
    runtime_type::{self, InvalidIdentifierKind},
};
use baml_type::{RealizedTy, Ty, TyAttr, normalize, normalize::TypeContext};
use bex_heap::TlabHolder;
use bex_vm_types::{
    AtomicValueSlot, HeapPtr, Object, RuntimeCompileArtifact, RuntimeSessionCompileArtifact,
    SessionEvalLease,
    link::link_dynamic,
    relink::{IndexOperand, visit_object_operands},
    types::{
        DynTypeDefs, LocalName, MethodImpl, Package, RuntimeImplRule, RuntimePackage,
        RuntimeTypeProvenance, SessionState, TypeValue, Value,
    },
};
use indexmap::IndexMap;

use super::{
    BamlClassReflectPackage, BamlClassReflectSession, BamlNamespaceReflect, Continuation,
    ImplResolver, NativeCallResult, PackageBamlImpl, PassThroughContinuation, copy,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
    vm::CallableSignature,
};

fn compilation_error(vm: &mut BexVm, id: DiagnosticId, message: String) -> VmRustFnError {
    let diagnostic = super::type_kinds::compiler_diagnostic(id, message);
    VmRustFnError::Thrown(super::type_kinds::alloc_compilation_error(
        vm,
        &[diagnostic],
    ))
}

/// Element tag for the `Arg[]` / `map<string, Arg>` containers. The class
/// instances themselves are built through the generated `copy::` structs.
const ARG_FQN: &str = "baml.reflect.Arg";

/// Materialize the public wrapper for the package selected by the lexical
/// `Package.current()` instruction. Dynamic code uses its owning package;
/// static code uses the package name baked at the call site.
pub(crate) fn current_package_value(vm: &mut BexVm, static_package: &str) -> Value {
    let runtime = vm.current_runtime_package();
    let package = if runtime.is_null() {
        vm.packages
            .package_ptr(&baml_type::Name::new(static_package))
            .expect("Package.current call site names a loaded package")
    } else {
        runtime
    };
    copy::reflect::Package {
        _inner: Value::object(package),
    }
    .to_value(vm)
}

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
    if matches!(vm.get_object(wrapper_ptr), Object::Package(_)) {
        return Ok(wrapper_ptr);
    }
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
    let path = path.strip_prefix("root.").unwrap_or(path);
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

fn display_local_name(name: &LocalName) -> String {
    std::iter::once("root")
        .chain(name.namespace.iter().map(baml_type::Name::as_str))
        .chain(std::iter::once(name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

fn runtime_type_key(name: &LocalName) -> String {
    name.namespace
        .iter()
        .map(baml_type::Name::as_str)
        .chain(std::iter::once(name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

fn stored_package_type(package: &Package, name: &LocalName) -> Option<HeapPtr> {
    name.namespace
        .is_empty()
        .then(|| package.mounted_types.get(name.name.as_str()).copied())
        .flatten()
        .or_else(|| {
            package
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.type_values.get(&runtime_type_key(name)).copied())
        })
}

/// Runtime type facts rooted at the Package being inspected, not at the code
/// that happened to call the reflection API. This distinction is observable
/// when a generated package's return class implements an interface imported
/// from one of its live dependencies.
struct PackageSubtypeContext<'a> {
    vm: &'a BexVm,
    package: HeapPtr,
}

impl TypeContext for PackageSubtypeContext<'_> {
    fn alias_def(&self, name: &baml_type::QualifiedTypeName) -> Option<Ty> {
        TypeContext::alias_def(self.vm, name)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &baml_type::Interface) -> bool {
        let Ok(concrete) = RealizedTy::try_from(concrete) else {
            return false;
        };
        let Ok(args) = interface
            .generics
            .iter()
            .map(RealizedTy::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let Ok(assoc) = interface
            .associated_types
            .iter()
            .map(|(name, ty)| RealizedTy::try_from(ty).map(|ty| (name.clone(), ty)))
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        ImplResolver::for_package(self.vm, self.package).type_implements(
            &concrete,
            &interface.name,
            &args,
            &assoc,
        )
    }

    fn type_var_bound(&self, param: &baml_type::ParamTy) -> Vec<baml_type::Interface> {
        TypeContext::type_var_bound(self.vm, param)
    }

    fn interface_requires(&self, sub: &baml_type::Interface, sup: &baml_type::Interface) -> bool {
        TypeContext::interface_requires(self.vm, sub, sup)
    }

    fn enum_variants(&self, name: &baml_type::QualifiedTypeName) -> Option<Vec<baml_type::Name>> {
        TypeContext::enum_variants(self.vm, name)
    }

    fn associated_type_bound(
        &self,
        interface: &baml_type::Interface,
        assoc: baml_type::Name,
    ) -> Vec<baml_type::Interface> {
        TypeContext::associated_type_bound(self.vm, interface, assoc)
    }

    fn project(
        &self,
        base: &Ty,
        interface: &baml_type::Interface,
        member: &baml_type::Name,
        fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        TypeContext::project(self.vm, base, interface, member, fuel)
    }
}

fn package_class_type(vm: &mut BexVm, runtime_type: Option<HeapPtr>, class_ptr: HeapPtr) -> Value {
    if let Some(runtime_type) = runtime_type {
        return Value::object(runtime_type);
    }
    let Object::Class(class) = vm.get_object(class_ptr) else {
        unreachable!("Package.classes only contains class pointers")
    };
    let ty = RealizedTy::Class(class.name.clone(), Vec::new(), class.ty_attr.clone());
    let mut defs = DynTypeDefs::default();
    defs.classes.insert(class.name.clone(), class_ptr);
    Value::object(vm.alloc_static_type_with_defs(ty, defs))
}

fn package_enum_type(vm: &mut BexVm, runtime_type: Option<HeapPtr>, enum_ptr: HeapPtr) -> Value {
    if let Some(runtime_type) = runtime_type {
        return Value::object(runtime_type);
    }
    let Object::Enum(enm) = vm.get_object(enum_ptr) else {
        unreachable!("Package.enums only contains enum pointers")
    };
    let ty = RealizedTy::Enum(enm.name.clone(), enm.ty_attr.clone());
    let defs = DynTypeDefs::with_enum(enm.name.clone(), enum_ptr);
    Value::object(vm.alloc_static_type_with_defs(ty, defs))
}

fn package_interface_type(
    vm: &mut BexVm,
    runtime_type: Option<HeapPtr>,
    interface_ptr: HeapPtr,
) -> Value {
    if let Some(runtime_type) = runtime_type {
        return Value::object(runtime_type);
    }
    let Object::Interface(interface) = vm.get_object(interface_ptr) else {
        unreachable!("Package.interfaces only contains interface pointers")
    };
    let ty = RealizedTy::Interface(
        interface.name.clone(),
        Vec::new(),
        Vec::new(),
        TyAttr::default(),
    );
    Value::object(vm.alloc_static_type(ty))
}

/// The definition overlay a package's own declarations form.
///
/// A statically compiled package needs none: its declarations live in the
/// engine image and resolve by name. A runtime package's do not, so every
/// `type` value reflection hands out of one has to carry this overlay or the
/// name it mentions is unresolvable — the BEP-066 rule that a minted type and
/// its definitions travel together.
fn declaration_defs(
    vm: &BexVm,
    classes: &IndexMap<LocalName, HeapPtr>,
    enums: &IndexMap<LocalName, HeapPtr>,
) -> DynTypeDefs {
    DynTypeDefs {
        classes: classes
            .values()
            .filter_map(|ptr| match vm.get_object(*ptr) {
                Object::Class(class) if !class.name.name().as_str().ends_with("$stream") => {
                    Some((class.name.clone(), *ptr))
                }
                _ => None,
            })
            .collect(),
        enums: enums
            .values()
            .filter_map(|ptr| match vm.get_object(*ptr) {
                Object::Enum(enm) => Some((enm.name.clone(), *ptr)),
                _ => None,
            })
            .collect(),
        witnesses: Vec::new(),
    }
}

/// [`declaration_defs`] for an already-populated package object. Empty for a
/// null pointer, which is what a statically compiled callable reports.
fn package_defs(vm: &BexVm, package_ptr: HeapPtr) -> DynTypeDefs {
    if package_ptr.is_null() {
        return DynTypeDefs::default();
    }
    let Object::Package(package) = vm.get_object(package_ptr) else {
        return DynTypeDefs::default();
    };
    declaration_defs(vm, &package.classes, &package.enums)
}

/// Allocate a `type` value inside a definition overlay, keeping the memoized
/// static path when there is nothing to carry.
fn alloc_type_in(vm: &mut BexVm, ty: RealizedTy, defs: &DynTypeDefs) -> Value {
    if defs.is_empty() {
        return Value::object(vm.alloc_static_type(ty));
    }
    Value::object(vm.alloc_static_type_with_defs(ty, defs.clone()))
}

fn allocate_runtime_declaration_types(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    classes: &IndexMap<LocalName, HeapPtr>,
    enums: &IndexMap<LocalName, HeapPtr>,
    interfaces: &IndexMap<LocalName, HeapPtr>,
) -> IndexMap<String, HeapPtr> {
    let source_defs = declaration_defs(vm, classes, enums);
    let class_rows = classes
        .iter()
        .filter_map(|(name, &class_ptr)| match vm.get_object(class_ptr) {
            Object::Class(class) => Some((
                runtime_type_key(name),
                class_ptr,
                RealizedTy::Class(class.name.clone(), Vec::new(), class.ty_attr.clone()),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let enum_rows = enums
        .iter()
        .filter_map(|(name, &enum_ptr)| match vm.get_object(enum_ptr) {
            Object::Enum(enm) => Some((
                runtime_type_key(name),
                enum_ptr,
                RealizedTy::Enum(enm.name.clone(), enm.ty_attr.clone()),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let interface_rows = interfaces
        .iter()
        .filter_map(
            |(name, &interface_ptr)| match vm.get_object(interface_ptr) {
                Object::Interface(interface) => Some((
                    runtime_type_key(name),
                    RealizedTy::Interface(
                        interface.name.clone(),
                        Vec::new(),
                        Vec::new(),
                        TyAttr::default(),
                    ),
                )),
                _ => None,
            },
        )
        .collect::<Vec<_>>();

    let mut type_values = IndexMap::new();
    for (name, class_ptr, ty) in class_rows {
        let mint = vm.heap.mint_runtime_id();
        let type_ptr = vm.alloc_type(TypeValue::runtime_with_defs(
            ty,
            mint,
            source_defs.clone(),
            package_ptr,
        ));
        let Object::Class(class) = vm.get_object_mut(class_ptr) else {
            unreachable!("runtime package class pointer changed kind")
        };
        class.runtime_type = Some(RuntimeTypeProvenance {
            mint,
            defs: source_defs.clone(),
            owner: package_ptr,
        });
        type_values.insert(name, type_ptr);
    }
    for (name, enum_ptr, ty) in enum_rows {
        let mint = vm.heap.mint_runtime_id();
        let type_ptr = vm.alloc_type(TypeValue::runtime_with_defs(
            ty,
            mint,
            source_defs.clone(),
            package_ptr,
        ));
        let Object::Enum(enm) = vm.get_object_mut(enum_ptr) else {
            unreachable!("runtime package enum pointer changed kind")
        };
        enm.runtime_type = Some(RuntimeTypeProvenance {
            mint,
            defs: source_defs.clone(),
            owner: package_ptr,
        });
        type_values.insert(name, type_ptr);
    }
    for (name, ty) in interface_rows {
        let mint = vm.heap.mint_runtime_id();
        let type_ptr = vm.alloc_type(TypeValue::runtime_with_defs(
            ty,
            mint,
            source_defs.clone(),
            package_ptr,
        ));
        type_values.insert(name, type_ptr);
    }
    type_values
}

fn package_function_value(vm: &mut BexVm, package_ptr: HeapPtr, name: &LocalName) -> Option<Value> {
    let local = name
        .namespace
        .iter()
        .map(baml_type::Name::as_str)
        .chain(std::iter::once(name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".");
    let (slot, runtime_package) = match vm.get_object(package_ptr) {
        Object::Package(package) => match package.runtime.as_ref() {
            Some(runtime) => {
                let slot = runtime
                    .global_names
                    .get(&format!("user.{local}"))
                    .or_else(|| runtime.global_names.get(&local));
                let slot = slot?;
                (bex_vm_types::GlobalIndex::from_raw(*slot), package_ptr)
            }
            None => {
                let package = vm.packages.package_name(package_ptr)?;
                let slot = vm.packages.global_by_name(&format!("{package}.{local}"))?;
                (slot, HeapPtr::null())
            }
        },
        _ => return None,
    };
    Some(Value::object(vm.alloc(Object::GenericFunction(
        bex_vm_types::GenericFunction {
            function: slot,
            type_args: Box::new([]),
            runtime_package,
            exact_type_values: None,
        },
    ))))
}

/// The descriptor `Package.functions()` hands out for one exported function.
///
/// A reconstructible signature becomes the descriptor's type, exactly as
/// before. A generic function whose declared surface still mentions its own
/// type parameters has no [`RealizedTy`] at all — the realized family excludes
/// type variables — so its descriptor denotes `unknown` and refuses
/// signature-shaped reads until it is specialized. Either way the callable
/// travels with the value, which is what `is_generic`, `generic_params`,
/// `specialize` and `get` read.
fn function_descriptor(vm: &mut BexVm, package: HeapPtr, name: &LocalName) -> Option<Value> {
    let callable = package_function_value(vm, package, name)?;
    let ty = match vm.callable_signature(callable) {
        Some(signature) => callee_fn_ty(&signature),
        // Anything else that fails to reconstruct is not a callable this
        // package can describe, and stays out of the listing as it always has.
        None if vm.unspecialized_generic_callable_name(callable).is_some() => RealizedTy::unknown(),
        None => return None,
    };
    let defs = package_defs(vm, package);
    Some(alloc_descriptor(vm, ty, defs, callable))
}

/// Allocate a descriptor `type` value for `callable`. A non-object callable
/// cannot happen (every function value is a heap wrapper) but degrades to a
/// plain `type` value rather than asserting.
fn alloc_descriptor(vm: &mut BexVm, ty: RealizedTy, defs: DynTypeDefs, callable: Value) -> Value {
    match callable.as_object_ptr() {
        Some(ptr) => Value::object(vm.alloc_function_descriptor(ty, defs, ptr)),
        None => alloc_type_in(vm, ty, &defs),
    }
}

/// The callable a function descriptor was reflected from, or `None` for an
/// ordinary function type value.
fn descriptor_callable(vm: &BexVm, r#type: Value) -> Option<Value> {
    let ptr = r#type.as_object_ptr()?;
    let Object::Type(type_value) = vm.get_object(ptr) else {
        return None;
    };
    (!type_value.callable.is_null()).then(|| Value::object(type_value.callable))
}

/// A descriptor's callable together with its declared type-parameter arity and
/// the arguments it already carries.
struct DescriptorCallable {
    value: Value,
    name: String,
    /// De Bruijn arity: how many type arguments a complete frame needs.
    declared: usize,
    /// The arguments the callable already carries.
    supplied: Vec<RealizedTy>,
}

/// The declared name of the callable behind a descriptor, for diagnostics.
pub(super) fn descriptor_callable_name(vm: &BexVm, r#type: Value) -> Option<String> {
    vm.callable_diagnostic_name(descriptor_callable(vm, r#type)?)
}

/// The exact `type` values a descriptor was specialized with.
///
/// A decomposing read (`params`, `return_type`) rebuilds the types it hands
/// back out of the descriptor's `RealizedTy`, which re-mints them statically.
/// That loses the identity of a runtime-minted argument, so a specialized
/// descriptor reports the very values it was given wherever they appear in its
/// signature — the same "hand back the exact value" rule `LoadType` applies to
/// a frame's type-argument slots and a runtime class field applies to its type.
/// Empty for everything that is not a specialized descriptor.
pub(super) fn descriptor_exact_type_values(vm: &BexVm, r#type: Value) -> Vec<TypeValue> {
    let Some(callable) = descriptor_callable(vm, r#type) else {
        return Vec::new();
    };
    let Some(ptr) = callable.as_object_ptr() else {
        return Vec::new();
    };
    let Object::GenericFunction(generic) = vm.get_object(ptr) else {
        return Vec::new();
    };
    generic
        .exact_type_values
        .iter()
        .flatten()
        .flatten()
        .cloned()
        .collect()
}

fn descriptor_callable_info(vm: &BexVm, r#type: Value) -> Option<DescriptorCallable> {
    let value = descriptor_callable(vm, r#type)?;
    let (function, supplied) = vm.callable_function_and_type_args(value)?;
    Some(DescriptorCallable {
        value,
        name: vm.callable_diagnostic_name(value)?,
        declared: function.generic_param_bounds.len(),
        supplied: supplied.to_vec(),
    })
}

/// Whether a descriptor still expects type arguments. The same arity question
/// [`BexVm::unspecialized_generic_callable_name`] asks, phrased for a value
/// that may not be a descriptor at all.
pub(super) fn descriptor_is_generic(vm: &BexVm, r#type: Value) -> bool {
    descriptor_callable_info(vm, r#type)
        .is_some_and(|callable| callable.supplied.len() < callable.declared)
}

/// The type parameters a descriptor still expects, in declaration order.
///
/// Names come from the callable's `display_type_params`, which render a bound
/// inline (`T extends Shape`); the bound is stripped here — `specialize`
/// enforces bounds, `generic_params` only names them.
pub(super) fn descriptor_generic_params(vm: &mut BexVm, r#type: Value) -> Vec<Value> {
    let Some(callable) = descriptor_callable_info(vm, r#type) else {
        return Vec::new();
    };
    let Some((function, _)) = vm.callable_function_and_type_args(callable.value) else {
        return Vec::new();
    };
    let names = function
        .display_type_params
        .iter()
        .enumerate()
        .skip(callable.supplied.len())
        .map(|(index, display)| {
            let name = display
                .split_once(" extends ")
                .map_or(display.as_str(), |(name, _)| name)
                .trim();
            // A synthesized frame slot can be nameless; report its De Bruijn
            // position rather than an empty string.
            if name.is_empty() {
                format!("#{index}")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>();
    names
        .into_iter()
        .map(|name| {
            let name = Value::object(vm.alloc_string(name.as_str()));
            copy::reflect::function::GenericParam { name }.to_value(vm)
        })
        .collect()
}

fn throw_diagnostic(
    vm: &mut BexVm,
    diagnostic: baml_compiler_diagnostics::Diagnostic,
) -> VmRustFnError {
    VmRustFnError::Thrown(super::type_kinds::alloc_compilation_error(
        vm,
        &[diagnostic],
    ))
}

/// `Shaped` / `Container<int>` — how a violated bound is named in a diagnostic.
/// Rendered as the interface *type* it denotes so it reads the same way as the
/// supplied type argument beside it, rather than as a raw qualified name.
fn render_bound(
    interface: &baml_type::TypeName,
    args: &[RealizedTy],
    assoc: &[(baml_type::Name, RealizedTy)],
) -> String {
    RealizedTy::Interface(
        interface.clone(),
        args.to_vec(),
        assoc.to_vec(),
        TyAttr::default(),
    )
    .to_string()
}

/// Bind a descriptor's remaining type parameters and return a specialized
/// descriptor.
///
/// The supplied `type` values are carried by reference onto the new callable
/// (`GenericFunction::exact_type_values`), never re-minted: a runtime type
/// keeps its identity and its definition overlay all the way into the callee's
/// frame, which is what lets the specialized body render its schema.
pub(super) fn descriptor_specialize(
    vm: &mut BexVm,
    r#type: Value,
    args: &[Value],
) -> Result<Value, VmRustFnError> {
    let Some(callable) = descriptor_callable_info(vm, r#type) else {
        return Err(throw_diagnostic(
            vm,
            runtime_type::specialize_without_descriptor(),
        ));
    };
    let bound_slot = callable.supplied.len();
    let remaining = callable.declared.saturating_sub(bound_slot);
    if remaining == 0 {
        return Err(throw_diagnostic(
            vm,
            runtime_type::specialize_non_generic(&callable.name),
        ));
    }
    if args.len() != remaining {
        return Err(throw_diagnostic(
            vm,
            runtime_type::specialize_arity_mismatch(&callable.name, remaining, args.len()),
        ));
    }
    // A specialized callable has to be re-wrappable, and only a
    // `GenericFunction` addresses its target by global slot. Every descriptor
    // this module hands out is one; a closure or bound method reaching here
    // would be a value reflection never produced.
    let Some(Object::GenericFunction(generic)) =
        callable.value.as_object_ptr().map(|ptr| vm.get_object(ptr))
    else {
        return Err(throw_diagnostic(
            vm,
            runtime_type::specialize_without_descriptor(),
        ));
    };
    let (slot, runtime_package) = (generic.function, generic.runtime_package);

    let supplied = args
        .iter()
        .map(|value| super::type_kinds::reflected_type_value(vm, *value))
        .collect::<Vec<_>>();
    let mut type_args = callable.supplied.clone();
    type_args.extend(supplied.iter().map(|value| value.ty.clone()));

    let (bounds, param_names) = {
        let Some((function, _)) = vm.callable_function_and_type_args(callable.value) else {
            return Err(throw_diagnostic(
                vm,
                runtime_type::specialize_without_descriptor(),
            ));
        };
        (
            function.generic_param_bounds.clone(),
            function.display_type_params.clone(),
        )
    };
    // The same proof the VM runs before entering a runtime-checked generic
    // call (`validate_runtime_generic_bounds`), rooted at the supplied value's
    // own dynamic world so a runtime-minted type's impls are visible, and
    // reported as a typed diagnostic instead of a bare "mismatched types".
    for (index, param_bounds) in bounds.iter().enumerate().skip(bound_slot) {
        let Some(actual) = type_args.get(index).cloned() else {
            continue;
        };
        let param_name = param_names.get(index).map_or_else(
            || format!("#{index}"),
            |display| {
                display
                    .split_once(" extends ")
                    .map_or(display.as_str(), |(name, _)| name)
                    .trim()
                    .to_string()
            },
        );
        for bound in param_bounds {
            let requested_args = bound
                .args
                .iter()
                .map(|arg| arg.substitute(&type_args, vm).ok())
                .collect::<Option<Vec<_>>>();
            let requested_assoc = bound
                .assoc
                .iter()
                .map(|(name, ty)| {
                    ty.substitute(&type_args, vm)
                        .ok()
                        .map(|ty| (name.clone(), ty))
                })
                .collect::<Option<Vec<_>>>();
            let satisfied = match (requested_args, requested_assoc) {
                (Some(bound_args), Some(bound_assoc)) => {
                    let holds = ImplResolver::for_value(vm, args[index - bound_slot])
                        .type_implements(&actual, &bound.interface, &bound_args, &bound_assoc);
                    if holds {
                        continue;
                    }
                    render_bound(&bound.interface, &bound_args, &bound_assoc)
                }
                // A bound that cannot be realized against a complete frame is
                // a bound this specialization cannot discharge. Report it as
                // the failure it is rather than raising an internal error.
                _ => bound.interface.to_string(),
            };
            let supplied_ty = actual.to_string();
            return Err(throw_diagnostic(
                vm,
                runtime_type::specialize_bound_violation(
                    &callable.name,
                    &param_name,
                    &satisfied,
                    &supplied_ty,
                ),
            ));
        }
    }

    let mut exact = vec![None; bound_slot];
    exact.extend(supplied.iter().cloned().map(Some));
    let specialized = Value::object(vm.alloc(Object::GenericFunction(
        bex_vm_types::GenericFunction {
            function: slot,
            type_args: type_args.into_boxed_slice(),
            runtime_package,
            exact_type_values: Some(exact.into_boxed_slice()),
        },
    )));

    // With every slot supplied the templates realize, so this is the whole of
    // "specialize" from the signature's point of view.
    let ty = match vm.callable_signature(specialized) {
        Some(signature) => callee_fn_ty(&signature),
        None => RealizedTy::unknown(),
    };
    let mut defs = super::type_kinds::reflected_type_value(vm, r#type)
        .defs()
        .clone();
    for value in &supplied {
        defs.merge_from(value.defs());
    }
    Ok(alloc_descriptor(vm, ty, defs, specialized))
}

/// Extract a descriptor's callable through the caller's `F` contract — the
/// descriptor-side twin of `Package.get_function`, for the specialized
/// descriptors that have no name to look up.
pub(super) fn descriptor_get(
    vm: &mut BexVm,
    r#type: Value,
) -> Result<Option<Value>, VmRustFnError> {
    let Some(callable) = descriptor_callable_info(vm, r#type) else {
        return Ok(None);
    };
    let Some(signature) = vm.callable_signature(callable.value) else {
        return Err(throw_diagnostic(
            vm,
            runtime_type::unspecialized_reflected_generic(&callable.name),
        ));
    };
    if let Some(name) = vm.generic_callable_body_needs_type_args(callable.value) {
        return Err(throw_diagnostic(
            vm,
            runtime_type::unspecialized_reflected_generic_call(&name),
        ));
    }
    let package = vm.callable_runtime_package(callable.value);
    check_function_contract(vm, package, &callable.name, &signature)?;
    Ok(Some(callable.value))
}

/// The `F` contract check shared by `Package.get_function` and a descriptor's
/// `get`: the callable's reconstructed function type must be a subtype of the
/// contract the caller asked for.
///
/// A runtime package roots the subtype context at itself so its own impls are
/// visible; a statically compiled callable has no such root and uses the VM's
/// lexical world.
fn check_function_contract(
    vm: &mut BexVm,
    package: HeapPtr,
    name: &str,
    signature: &CallableSignature,
) -> Result<(), VmRustFnError> {
    let actual = callee_fn_ty(signature);
    let expected = vm
        .current_call_type_args()
        .first()
        .cloned()
        .unwrap_or_else(RealizedTy::unknown);
    let sub = Ty::from(actual.clone());
    let sup = Ty::from(expected.clone());
    let matches = if package.is_null() {
        normalize::is_subtype(&sub, &sup, vm)
    } else {
        normalize::is_subtype(&sub, &sup, &PackageSubtypeContext { vm, package })
    };
    if matches {
        return Ok(());
    }
    Err(compilation_error(
        vm,
        DiagnosticId::TypeMismatch,
        format!(
            "function `{name}` has type `{actual}`, which is not a subtype of requested contract `{expected}`"
        ),
    ))
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
        .or_else(|| package.functions.get(&local_name))
        .copied()
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
        span,
        message,
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

fn test_function_ty() -> RealizedTy {
    RealizedTy::Function {
        params: Vec::new(),
        ret: Box::new(RealizedTy::null()),
        throws: Box::new(RealizedTy::unknown()),
        attr: TyAttr::default(),
    }
}

fn empty_tests(vm: &mut BexVm) -> Value {
    Value::object(
        vm.tlab
            .alloc_map(RealizedTy::string(), test_function_ty(), IndexMap::new()),
    )
}

struct RegisterPackageTests {
    test_init: HeapPtr,
}

impl Continuation for RegisterPackageTests {
    fn call(self: Box<Self>, _vm: &mut BexVm, collector: Value) -> NativeCallResult {
        let Some(collector_ptr) = collector.as_object_ptr() else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "testing.TestCollector.new returned a non-instance".to_string(),
            })
            .into();
        };
        NativeCallResult::YieldToCall {
            callee: self.test_init,
            args: vec![collector],
            type_args: Vec::new(),
            continuation: Box::new(FinishPackageTests { collector_ptr }),
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.test_init]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.test_init) {
            self.test_init = ptr;
        }
    }
}

struct FinishPackageTests {
    collector_ptr: HeapPtr,
}

impl Continuation for FinishPackageTests {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        let tests_value = match vm.get_object(self.collector_ptr) {
            Object::Instance(collector) => collector.load_field(1),
            _ => {
                return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                    message: "test collector changed kind during registration".to_string(),
                })
                .into();
            }
        };
        let registrations = match vm.as_array(&tests_value) {
            Ok(tests) => tests.to_vec(),
            Err(error) => return error.into(),
        };
        let mut tests = IndexMap::new();
        for registration in registrations {
            let Some(ptr) = registration.as_object_ptr() else {
                continue;
            };
            let Object::Instance(registration) = vm.get_object(ptr) else {
                continue;
            };
            let name_value = registration.load_field(0);
            let body = registration.load_field(1);
            let Ok(name) = vm.as_string(&name_value) else {
                continue;
            };
            tests.insert(name.clone(), body);
        }
        NativeCallResult::Done(Value::object(vm.tlab.alloc_map(
            RealizedTy::string(),
            test_function_ty(),
            tests,
        )))
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        vec![self.collector_ptr]
    }

    fn apply_forwarding(&mut self, forwarding: &std::collections::HashMap<HeapPtr, HeapPtr>) {
        if let Some(&ptr) = forwarding.get(&self.collector_ptr) {
            self.collector_ptr = ptr;
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
            // Keep runtime rejection single-sourced with compiler mount filtering.
            if baml_builtins2::reserved_package_names().contains(&alias.as_str()) {
                let diagnostic = super::type_kinds::compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("package alias `{alias}` is reserved"),
                );
                return VmRustFnError::Thrown(super::type_kinds::alloc_compilation_error(
                    vm,
                    &[diagnostic],
                ))
                .into();
            }
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
            exported_names: program_package.exported_names.clone(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            functions: IndexMap::new(),
            recursive_type_aliases: program_package.recursive_type_aliases.clone(),
            interface_blob: artifact.interface_blob,
            test_init: None,
            mounted_types: IndexMap::new(),
            runtime: Some(Box::new(RuntimePackage {
                objects: Box::new([]),
                object_names: IndexMap::new(),
                globals: Box::new([]),
                global_names: IndexMap::new(),
                type_values: IndexMap::new(),
                diagnostics: artifact.diagnostics,
                dependencies: dependencies.values().copied().collect(),
                dependency_names: dependencies.clone(),
                init: None,
                initialized: false,
            })),
            session: None,
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
                let qtn = baml_type::QualifiedTypeName::from_dotted_path(&symbol.fq_name);
                vm.dynamic_dispatch
                    .class_ptr(&qtn)
                    .or_else(|| {
                        dependencies.values().find_map(|package_ptr| {
                            let Object::Package(package) = vm.get_object(*package_ptr) else {
                                return None;
                            };
                            package.mounted_types.values().find_map(|type_ptr| {
                                let Object::Type(value) = vm.get_object(*type_ptr) else {
                                    return None;
                                };
                                value
                                    .defs()
                                    .classes
                                    .get(&qtn)
                                    .or_else(|| value.defs().enums.get(&qtn))
                                    .copied()
                            })
                        })
                    })
                    .or_else(|| vm.packages.object_by_name(&symbol.fq_name))
                    .or_else(|| {
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
            .enumerate()
            // External prefix entries are already-live functions. Their
            // constants are resolved against the owning static/dynamic image,
            // not against this newly linked candidate's object vector.
            .filter(|(index, _)| !external_objects.contains_key(index))
            .map(|(_, ptr)| *ptr)
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
                    | bex_vm_types::ConstValue::ClassWithTypeArgs { .. }
                    | bex_vm_types::ConstValue::Literal(_) => Value::NULL,
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
                        if let Some(runtime) = package.runtime.as_ref() {
                            let index = runtime
                                .global_names
                                .get(&format!("user.{local}"))
                                .or_else(|| runtime.global_names.get(local))?;
                            runtime.load_global(*index)
                        } else {
                            let canonical = vm.packages.package_name(*dependencies.get(alias)?)?;
                            let index = vm
                                .packages
                                .global_by_name(&format!("{canonical}.{local}"))?;
                            Some(vm.globals.get(vm.proof(), index))
                        }
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
        let functions = program_package
            .functions
            .iter()
            .map(|(name, index)| (name.clone(), objects[index.raw()]))
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
        let test_init = program_package.test_init.map(|index| objects[index.raw()]);

        let type_values =
            allocate_runtime_declaration_types(vm, package_ptr, &classes, &enums, &interfaces);

        let Object::Package(package) = vm.get_object_mut(package_ptr) else {
            unreachable!("package was just allocated")
        };
        package.classes = classes;
        package.enums = enums;
        package.interfaces = interfaces;
        package.impl_rules = impl_rules;
        package.functions = functions;
        package.test_init = test_init;
        let runtime = package.runtime.as_mut().expect("runtime package image");
        runtime.objects = objects.into_boxed_slice();
        runtime.globals = globals.into_boxed_slice();
        runtime.global_names = global_names;
        runtime.type_values = type_values;
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
        let local = local_name(name.as_str())?;
        if local.name.as_str().ends_with("$stream") {
            return None;
        }
        let class_ptr = package.classes.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_class_type(vm, runtime_type, class_ptr))
    }

    fn get_enum(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let local = local_name(name.as_str())?;
        let enum_ptr = package.enums.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_enum_type(vm, runtime_type, enum_ptr))
    }

    fn get_interface(vm: &mut BexVm, package: &Value, name: &bex_str::BexStr) -> Option<Value> {
        let ptr = package_ptr(vm, *package).ok()?;
        let Object::Package(package) = vm.get_object(ptr) else {
            return None;
        };
        let local = local_name(name.as_str())?;
        let interface_ptr = package.interfaces.get(&local).copied()?;
        let runtime_type = stored_package_type(package, &local);
        Some(package_interface_type(vm, runtime_type, interface_ptr))
    }

    fn with_types(
        vm: &mut BexVm,
        package: &Value,
        types: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, VmRustFnError> {
        let source_ptr = package_ptr(vm, *package)?;
        let Object::Package(source) = vm.get_object(source_ptr) else {
            unreachable!("package_ptr validates Object::Package")
        };
        let mut derived = (**source).clone();

        for (export, value) in types {
            let export = export.to_string();
            if !super::type_kinds::is_baml_identifier(&export) {
                let diagnostic =
                    runtime_type::invalid_identifier(InvalidIdentifierKind::ExportedType, &export)
                        .with_phase(DiagnosticPhase::Hir);
                return Err(VmRustFnError::Thrown(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            let local = LocalName {
                namespace: Vec::new(),
                name: baml_type::Name::new(&export),
            };
            if derived.classes.contains_key(&local)
                || derived.enums.contains_key(&local)
                || derived.interfaces.contains_key(&local)
                || derived.exported_names.contains(&local)
                || derived.mounted_types.contains_key(&export)
            {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::DuplicateName,
                    format!("duplicate exported type name `{export}`"),
                ));
            }
            let Some(type_ptr) = value.as_object_ptr() else {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::TypeMismatch,
                    format!("with_types value for `{export}` must be a type"),
                ));
            };
            let Object::Type(type_value) = vm.get_object(type_ptr) else {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::TypeMismatch,
                    format!("with_types value for `{export}` must be a type"),
                ));
            };
            match &type_value.ty {
                RealizedTy::Class(qtn, _, _) => {
                    if let Some(class) = type_value
                        .defs()
                        .classes
                        .get(qtn)
                        .copied()
                        .or_else(|| vm.dynamic_dispatch.class_ptr(qtn))
                    {
                        derived.classes.insert(local.clone(), class);
                    }
                }
                RealizedTy::Enum(qtn, _) => {
                    if let Some(enm) = type_value.defs().enums.get(qtn).copied() {
                        derived.enums.insert(local.clone(), enm);
                    }
                }
                RealizedTy::Interface(qtn, _, _, _) => {
                    let owned_interface = (!type_value.owner.is_null())
                        .then(|| match vm.get_object(type_value.owner) {
                            Object::Package(owner) => owner.interfaces.values().find_map(|ptr| {
                                matches!(vm.get_object(*ptr), Object::Interface(interface) if interface.name == *qtn)
                                    .then_some(*ptr)
                            }),
                            _ => None,
                        })
                        .flatten();
                    if let Some(interface) = owned_interface.or_else(|| vm.lookup_interface(qtn)) {
                        derived.interfaces.insert(local.clone(), interface);
                    }
                }
                _ => {}
            }
            derived.mounted_types.insert(export, type_ptr);
            derived.exported_names.push(local);
        }

        let derived_ptr = vm.alloc(Object::Package(Box::new(derived)));
        Ok(copy::reflect::Package {
            _inner: Value::object(derived_ptr),
        }
        .to_value(vm))
    }

    fn get_function(
        vm: &mut BexVm,
        package: &Value,
        name: &bex_str::BexStr,
    ) -> Result<Option<Value>, VmRustFnError> {
        let package_ptr = package_ptr(vm, *package)?;
        let Some(local) = local_name(name.as_str()) else {
            return Ok(None);
        };
        let function = match vm.get_object(package_ptr) {
            Object::Package(package) => package.functions.get(&local).copied(),
            _ => None,
        };
        if function.is_none() {
            return Ok(None);
        }
        let Some(function_value) = package_function_value(vm, package_ptr, &local) else {
            return Ok(None);
        };
        let Some(signature) = vm.callable_signature(function_value) else {
            if vm
                .unspecialized_generic_callable_name(function_value)
                .is_some()
            {
                let diagnostic =
                    runtime_type::unspecialized_reflected_generic(&display_local_name(&local));
                return Err(VmRustFnError::Thrown(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            return Ok(None);
        };
        // Signature reconstruction only sees the declared surface. A companion
        // whose surface is free of its parent's `T` gets this far and would be
        // handed out as an ordinary function value — and calling one directly
        // fails inside its body as a VM internal error no `catch` can see. Refuse
        // it here, where the caller still has a diagnostic channel.
        if let Some(name) = vm.generic_callable_body_needs_type_args(function_value) {
            let diagnostic = runtime_type::unspecialized_reflected_generic_call(&name);
            return Err(VmRustFnError::Thrown(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        check_function_contract(vm, package_ptr, name.as_str(), &signature)?;
        Ok(Some(function_value))
    }

    fn classes(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .classes
            .iter()
            .filter(|(name, _)| !name.name.as_str().ends_with("$stream"))
            .map(|(name, &class)| (name.clone(), class, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, class, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_class_type(vm, runtime_type, class),
                )
            })
            .collect()
    }

    fn enums(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .enums
            .iter()
            .map(|(name, &enm)| (name.clone(), enm, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, enm, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_enum_type(vm, runtime_type, enm),
                )
            })
            .collect()
    }

    fn interfaces(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let entries = package
            .interfaces
            .iter()
            .map(|(name, &interface)| (name.clone(), interface, stored_package_type(package, name)))
            .collect::<Vec<_>>();
        entries
            .into_iter()
            .map(|(name, interface, runtime_type)| {
                (
                    display_local_name(&name).into(),
                    package_interface_type(vm, runtime_type, interface),
                )
            })
            .collect()
    }

    fn functions(vm: &mut BexVm, package: &Value) -> IndexMap<bex_str::BexStr, Value> {
        let Ok(ptr) = package_ptr(vm, *package) else {
            return IndexMap::new();
        };
        let Object::Package(package) = vm.get_object(ptr) else {
            return IndexMap::new();
        };
        let functions = package.functions.keys().cloned().collect::<Vec<_>>();
        functions
            .into_iter()
            .filter_map(|name| {
                function_descriptor(vm, ptr, &name)
                    .map(|descriptor| (display_local_name(&name).into(), descriptor))
            })
            .collect()
    }

    fn tests(vm: &mut BexVm, package: &Value) -> NativeCallResult {
        let package_ptr = match package_ptr(vm, *package) {
            Ok(package) => package,
            Err(error) => return error.into(),
        };
        let test_init = match vm.get_object(package_ptr) {
            Object::Package(package) => package.test_init,
            _ => None,
        };
        let Some(test_init) = test_init else {
            return NativeCallResult::Done(empty_tests(vm));
        };
        let Some(constructor) = vm.packages.object_by_name("testing.TestCollector.new") else {
            return NativeCallResult::Done(empty_tests(vm));
        };
        let prefix = Value::object(vm.alloc_string(""));
        NativeCallResult::YieldToCall {
            callee: constructor,
            args: vec![prefix],
            type_args: Vec::new(),
            continuation: Box::new(RegisterPackageTests { test_init }),
        }
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

fn session_external_object(
    vm: &BexVm,
    session: HeapPtr,
    dependencies: &IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<HeapPtr> {
    if let Object::Package(package) = vm.get_object(session)
        && let Some(pointer) = package
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.object_names.get(name))
    {
        return Some(*pointer);
    }
    vm.packages.object_by_name(name).or_else(|| {
        let (alias, local) = name.split_once('.')?;
        dependency_object(vm, *dependencies.get(alias)?, local)
    })
}

fn session_external_global(
    vm: &BexVm,
    session: HeapPtr,
    dependencies: &IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<Value> {
    if let Object::Package(package) = vm.get_object(session)
        && let Some(runtime) = package.runtime.as_ref()
        && let Some(index) = runtime.global_names.get(name)
    {
        return runtime.load_global(*index);
    }
    vm.packages
        .global_by_name(name)
        .map(|index| vm.globals.get(vm.proof(), index))
        .or_else(|| {
            let (alias, local) = name.split_once('.')?;
            let dependency = *dependencies.get(alias)?;
            let Object::Package(package) = vm.get_object(dependency) else {
                return None;
            };
            if let Some(runtime) = package.runtime.as_ref() {
                let index = runtime
                    .global_names
                    .get(&format!("user.{local}"))
                    .or_else(|| runtime.global_names.get(local))?;
                runtime.load_global(*index)
            } else {
                let package_name = vm.packages.package_name(dependency)?;
                let index = vm
                    .packages
                    .global_by_name(&format!("{package_name}.{local}"))?;
                Some(vm.globals.get(vm.proof(), index))
            }
        })
}

#[derive(Clone, Copy)]
struct SessionAction {
    helper: HeapPtr,
    target: usize,
    step: Option<usize>,
}

struct SessionExecution {
    package: HeapPtr,
    actions: Vec<SessionAction>,
    current: usize,
    metadata: RuntimeSessionCompileArtifact,
    result: Value,
    lease: SessionEvalLease,
}

impl Drop for SessionExecution {
    fn drop(&mut self) {
        // A callback throw unwinds and drops the native continuation without
        // calling it again. Release here as well as on normal completion so
        // the RustData artifact's cloned lease cannot leave the Session busy.
        self.lease.release();
    }
}

impl SessionExecution {
    fn next(self: Box<Self>) -> NativeCallResult {
        let action = self.actions[self.current];
        NativeCallResult::YieldToCall {
            callee: action.helper,
            args: Vec::new(),
            type_args: Vec::new(),
            continuation: self,
        }
    }
}

impl Continuation for SessionExecution {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let action = self.actions[self.current];
        {
            let Object::Package(package) = vm.get_object_mut(self.package) else {
                unreachable!("Session continuation retained its package")
            };
            let runtime = package.runtime.as_mut().expect("Session runtime image");
            runtime.globals[action.target].store(value);

            if let Some(step_index) = action.step {
                let step = &self.metadata.steps[step_index];
                let state = package.session.as_mut().expect("Session state");
                if let Some(source) = &step.replay_source {
                    state
                        .history
                        .entry(self.metadata.submission_name.clone())
                        .or_default()
                        .push_str(source);
                }
                if let Some((name, symbol)) = &step.binding {
                    state.visible.insert(name.clone(), symbol.clone());
                }
                if step.returns_value {
                    self.result = value;
                }
            }
        }
        vm.heap.write_barrier(self.package, value);

        self.current += 1;
        if self.current < self.actions.len() {
            return self.next();
        }
        self.lease.release();
        NativeCallResult::Done(self.result)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        std::iter::once(self.package)
            .chain(self.actions.iter().map(|action| action.helper))
            .chain(self.result.as_object_ptr())
            .collect()
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(&pointer) = forwarding.get(&self.package) {
            self.package = pointer;
        }
        for action in &mut self.actions {
            if let Some(&pointer) = forwarding.get(&action.helper) {
                action.helper = pointer;
            }
        }
        if let Some(pointer) = self.result.as_object_ptr()
            && let Some(&forwarded) = forwarding.get(&pointer)
        {
            self.result = Value::object(forwarded);
        }
    }
}

fn graft_session_submission(
    vm: &mut BexVm,
    package_ptr: HeapPtr,
    artifact: &RuntimeCompileArtifact,
    metadata: &RuntimeSessionCompileArtifact,
) -> Result<Vec<SessionAction>, VmRustFnError> {
    let plan = link_dynamic(&artifact.units).map_err(|error| VmBamlError::InvalidArgument {
        message: format!("Session runtime link failed: {error}"),
    })?;
    let (dependencies, existing_globals, existing_objects, existing_len, runtime_objects) = {
        let Object::Package(package) = vm.get_object(package_ptr) else {
            unreachable!("Session payload is a Package")
        };
        let runtime = package.runtime.as_ref().expect("Session runtime image");
        (
            runtime.dependency_names.clone(),
            runtime.global_names.clone(),
            runtime.object_names.clone(),
            runtime.globals.len(),
            runtime.objects.to_vec(),
        )
    };
    let external_objects = plan
        .external_objects
        .iter()
        .map(|(index, symbol)| (index.raw(), symbol))
        .collect::<HashMap<_, _>>();
    let external_globals = plan
        .external_globals
        .iter()
        .map(|(index, symbol)| (index.raw(), symbol))
        .collect::<HashMap<_, _>>();

    // Assign stable Session slots first. Imports of prior Session cells reuse
    // their old slot; every new definition gets a fresh slot, which is the
    // cell/shadowing boundary.
    let mut global_map = vec![0usize; plan.program.globals.len()];
    let mut next_global = existing_len;
    let mut imported_values = HashMap::<usize, Value>::new();
    let mut cached_import_names = Vec::<(String, usize)>::new();
    for (plan_index, stable_slot) in global_map.iter_mut().enumerate() {
        if let Some(symbol) = external_globals.get(&plan_index) {
            if let Some(&stable) = existing_globals.get(&symbol.fq_name) {
                *stable_slot = stable;
                continue;
            }
            let value = session_external_global(vm, package_ptr, &dependencies, &symbol.fq_name)
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: format!("Session link could not resolve global `{}`", symbol.fq_name),
                })?;
            *stable_slot = next_global;
            imported_values.insert(plan_index, value);
            cached_import_names.push((symbol.fq_name.clone(), next_global));
            next_global += 1;
        } else {
            *stable_slot = next_global;
            next_global += 1;
        }
    }

    // Allocate all owned objects before patching any object operands so cycles
    // and mutually-recursive declarations have a complete pointer map.
    let mut objects = Vec::with_capacity(plan.program.objects.len());
    let mut owned = Vec::new();
    for (index, object) in plan.program.objects.iter().enumerate() {
        let external = external_objects.get(&index).and_then(|symbol| {
            (!matches!(symbol.kind, bex_vm_types::SymbolKind::GenericFn))
                .then(|| {
                    existing_objects.get(&symbol.fq_name).copied().or_else(|| {
                        session_external_object(vm, package_ptr, &dependencies, &symbol.fq_name)
                    })
                })
                .flatten()
        });
        if let Some(pointer) = external {
            objects.push(pointer);
        } else {
            let pointer = vm.alloc(object.clone());
            objects.push(pointer);
            owned.push(pointer);
        }
    }
    let mut object_map = vec![0usize; objects.len()];
    let mut appended_objects = Vec::new();
    for (index, pointer) in objects.iter().copied().enumerate() {
        if let Some(stable) = runtime_objects
            .iter()
            .position(|existing| *existing == pointer)
        {
            object_map[index] = stable;
        } else {
            object_map[index] = runtime_objects.len() + appended_objects.len();
            appended_objects.push(pointer);
        }
    }
    let stable_objects = runtime_objects
        .iter()
        .copied()
        .chain(appended_objects.iter().copied())
        .collect::<Vec<_>>();
    for pointer in &owned {
        let object = vm.get_object_mut(*pointer);
        visit_object_operands(object, |operand| match operand {
            IndexOperand::Object(index) => {
                *index = bex_vm_types::ObjectIndex::from_raw(object_map[index.raw()]);
            }
            IndexOperand::Global(index) => {
                *index = bex_vm_types::GlobalIndex::from_raw(global_map[index.raw()]);
            }
        });
        match object {
            Object::Function(function) => {
                function.runtime_package = package_ptr;
                function.bytecode.compact = Some(function.bytecode.lower_to_compact());
            }
            Object::GenericFunction(function) => function.runtime_package = package_ptr,
            _ => {}
        }
    }

    let function_ptrs = owned
        .iter()
        .copied()
        .filter(|pointer| matches!(vm.get_object(*pointer), Object::Function(_)))
        .collect::<Vec<_>>();
    let mut extra_owned = Vec::new();
    for function_ptr in function_ptrs {
        let constants = match vm.get_object(function_ptr) {
            Object::Function(function) => function.bytecode.constants.clone(),
            _ => unreachable!(),
        };
        let mut resolved = Vec::with_capacity(constants.len());
        for constant in constants {
            let value = match constant {
                bex_vm_types::ConstValue::Type(_)
                | bex_vm_types::ConstValue::ClassWithTypeArgs { .. }
                | bex_vm_types::ConstValue::Literal(_) => Value::NULL,
                bex_vm_types::ConstValue::Float(value) => {
                    let pointer = vm.alloc_float(value);
                    extra_owned.push(pointer);
                    Value::object(pointer)
                }
                other => other.to_value(|index| stable_objects[index.raw()]),
            };
            resolved.push(value);
        }
        let Object::Function(function) = vm.get_object_mut(function_ptr) else {
            unreachable!()
        };
        function.bytecode.resolved_constants = resolved;
    }

    let mut appended = vec![Value::NULL; next_global - existing_len];
    for (plan_index, constant) in plan.program.globals.iter().enumerate() {
        let stable = global_map[plan_index];
        if stable < existing_len {
            continue;
        }
        let value = if let Some(value) = imported_values.get(&plan_index) {
            *value
        } else {
            match constant {
                bex_vm_types::ConstValue::Float(value) => {
                    let pointer = vm.alloc_float(*value);
                    extra_owned.push(pointer);
                    Value::object(pointer)
                }
                other => other.to_value(|index| objects[index.raw()]),
            }
        };
        appended[stable - existing_len] = value;
    }

    let program_package = plan
        .program
        .packages
        .get(&baml_type::Name::new("user"))
        .cloned()
        .unwrap_or_default();
    let new_classes = program_package
        .classes
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_enums = program_package
        .enums
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_interfaces = program_package
        .interfaces
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let new_type_values = allocate_runtime_declaration_types(
        vm,
        package_ptr,
        &new_classes,
        &new_enums,
        &new_interfaces,
    );
    let new_functions = program_package
        .functions
        .iter()
        .map(|(name, index)| (name.clone(), objects[index.raw()]))
        .collect::<IndexMap<_, _>>();
    let mut new_impl_rules = IndexMap::<HeapPtr, Vec<HeapPtr>>::new();
    for (interface_index, rules) in &program_package.impl_rules {
        let interface = objects[interface_index.raw()];
        let mut pointers = Vec::new();
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
            extra_owned.push(pointer);
            pointers.push(pointer);
        }
        new_impl_rules.insert(interface, pointers);
    }

    let mut object_name_updates = IndexMap::new();
    for (name, index) in &plan.program.function_indices {
        if name.starts_with("user.") {
            object_name_updates.insert(name.clone(), objects[*index]);
        }
    }
    for (name, pointer) in new_classes.iter().chain(&new_enums).chain(&new_interfaces) {
        object_name_updates.insert(
            format!(
                "user.{}",
                display_local_name(name).trim_start_matches("root.")
            ),
            *pointer,
        );
    }
    let global_name_updates = plan
        .program
        .function_global_indices
        .iter()
        .chain(&plan.program.let_global_indices)
        .filter(|(name, _)| name.starts_with("user."))
        .map(|(name, index)| (name.clone(), global_map[*index]))
        .collect::<Vec<_>>();

    let named_slots = plan
        .program
        .function_global_indices
        .values()
        .chain(plan.program.let_global_indices.values())
        .copied()
        .collect::<HashSet<_>>();
    let helper_globals = plan
        .program
        .globals
        .iter()
        .enumerate()
        .filter(|(index, _)| !named_slots.contains(index))
        .filter_map(|(_, value)| match value {
            bex_vm_types::ConstValue::Object(index) => match &plan.program.objects[*index] {
                Object::Function(function) if function.source_file == metadata.submission_name => {
                    Some(objects[index.raw()])
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let step_by_global = metadata
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (step.global.as_str(), index))
        .collect::<HashMap<_, _>>();
    let actions = metadata
        .initializers
        .iter()
        .map(|initializer| {
            let helper = helper_globals
                .get(initializer.helper_slot as usize)
                .copied()
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: "Session initializer helper was not linked".to_string(),
                })?;
            let plan_global = plan
                .program
                .let_global_indices
                .get(&initializer.target_global)
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: format!(
                        "Session initializer target `{}` was not linked",
                        initializer.target_global
                    ),
                })?;
            let step = step_by_global
                .get(initializer.target_global.as_str())
                .copied();
            let target = step
                .and_then(|index| metadata.steps[index].commit_global.as_ref())
                .and_then(|name| existing_globals.get(name))
                .copied()
                .unwrap_or(global_map[*plan_global]);
            Ok(SessionAction {
                helper,
                target,
                step,
            })
        })
        .collect::<Result<Vec<_>, VmBamlError>>()?;
    let Object::Package(package) = vm.get_object_mut(package_ptr) else {
        unreachable!()
    };
    package.classes.extend(new_classes);
    package.enums.extend(new_enums);
    package.interfaces.extend(new_interfaces);
    package.functions.extend(new_functions);
    package
        .recursive_type_aliases
        .extend(program_package.recursive_type_aliases);
    for (interface, rules) in new_impl_rules {
        package
            .impl_rules
            .entry(interface)
            .or_default()
            .extend(rules);
    }
    package.interface_blob.clone_from(&artifact.interface_blob);
    let state = package.session.as_mut().expect("Session state");
    if !metadata.declaration_source.trim().is_empty() {
        state.history.insert(
            metadata.submission_name.clone(),
            metadata.declaration_source.clone(),
        );
    }
    state.visible.extend(metadata.declarations.clone());
    let runtime = package.runtime.as_mut().expect("Session runtime image");
    let mut globals = runtime.globals.to_vec();
    globals.extend(appended.into_iter().map(AtomicValueSlot::new));
    runtime.globals = globals.into_boxed_slice();
    let mut retained_objects = runtime.objects.to_vec();
    retained_objects.extend(appended_objects);
    retained_objects.extend(extra_owned);
    runtime.objects = retained_objects.into_boxed_slice();
    runtime.object_names.extend(object_name_updates);
    runtime.global_names.extend(cached_import_names);
    runtime.global_names.extend(global_name_updates);
    // Every declaration submission is generative: its created-once Type value
    // carries a fresh mint and points back to the Session package that owns its
    // definitions. Overwriting a visible name updates only the newest lookup;
    // values and functions from older submissions retain their original mint.
    runtime.type_values.extend(new_type_values);
    runtime.diagnostics.clone_from(&artifact.diagnostics);
    Ok(actions)
}

impl BamlClassReflectSession for PackageBamlImpl {
    fn _new(
        vm: &mut BexVm,
        packages: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, VmRustFnError> {
        let mut dependencies = IndexMap::new();
        for (alias, value) in packages {
            // Keep runtime rejection single-sourced with compiler mount filtering.
            if baml_builtins2::reserved_package_names().contains(&alias.as_str()) {
                let diagnostic = super::type_kinds::compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("package alias `{alias}` is reserved"),
                );
                return Err(VmRustFnError::Thrown(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            dependencies.insert(alias.to_string(), package_ptr(vm, *value)?);
        }
        let package = Package {
            exported_names: Vec::new(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            functions: IndexMap::new(),
            recursive_type_aliases: IndexMap::new(),
            interface_blob: Vec::new(),
            test_init: None,
            mounted_types: IndexMap::new(),
            runtime: Some(Box::new(RuntimePackage {
                objects: Box::new([]),
                object_names: IndexMap::new(),
                globals: Box::new([]),
                global_names: IndexMap::new(),
                type_values: IndexMap::new(),
                diagnostics: Vec::new(),
                dependencies: dependencies.values().copied().collect(),
                dependency_names: dependencies,
                init: None,
                // Session cells intentionally stay mutable between evals.
                initialized: false,
            })),
            session: Some(Box::new(SessionState {
                history: IndexMap::new(),
                visible: IndexMap::new(),
                busy: Arc::new(AtomicBool::new(false)),
                submission_counter: 0,
            })),
        };
        let package = vm.alloc(Object::Package(Box::new(package)));
        Ok(copy::reflect::Session {
            _inner: Value::object(package),
        }
        .to_value(vm))
    }

    fn _finish(vm: &mut BexVm, session: &Value, artifact: &Value) -> NativeCallResult {
        let package = match package_ptr(vm, *session) {
            Ok(package) => package,
            Err(error) => return error.into(),
        };
        let Some(artifact_inner) =
            artifact
                .as_object_ptr()
                .and_then(|pointer| match vm.get_object(pointer) {
                    Object::Instance(instance) => Some(instance.load_field(0)),
                    _ => None,
                })
        else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "Session._finish received an invalid artifact".to_string(),
            })
            .into();
        };
        let mut artifact = match vm.as_rust_data::<RuntimeCompileArtifact>(&artifact_inner) {
            Ok(artifact) => artifact.clone(),
            Err(error) => return error.into(),
        };
        let Some(metadata) = artifact.session.clone() else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "Session._finish received a Package.compile artifact".to_string(),
            })
            .into();
        };
        let Some(lease) = artifact.session_lease.take() else {
            return VmRustFnError::BamlError(VmBamlError::InvalidArgument {
                message: "Session artifact has already been consumed".to_string(),
            })
            .into();
        };
        let actions = match graft_session_submission(vm, package, &artifact, &metadata) {
            Ok(actions) => actions,
            Err(error) => {
                lease.release();
                return error.into();
            }
        };
        if actions.is_empty() {
            lease.release();
            return NativeCallResult::Done(Value::NULL);
        }
        Box::new(SessionExecution {
            package,
            actions,
            current: 0,
            metadata,
            result: Value::NULL,
            lease,
        })
        .next()
    }

    fn diagnostics(vm: &mut BexVm, session: &Value) -> Vec<Value> {
        let Ok(package) = package_ptr(vm, *session) else {
            return Vec::new();
        };
        let diagnostics = match vm.get_object(package) {
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
    defs: &DynTypeDefs,
) -> Value {
    let name = match name {
        Some(n) => Value::object(vm.alloc_string(n.as_str())),
        None => Value::object(vm.alloc_string(format!("$arg{position}"))),
    };
    let ty = alloc_type_in(vm, ty, defs);
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
    // A signature reconstructed off a runtime package's function names that
    // package's classes and enums; the reported types have to carry its
    // declarations or nothing downstream can resolve them.
    let defs = package_defs(vm, vm.callable_runtime_package(f_val));
    let mut positional = Vec::new();
    let mut opts: IndexMap<bex_str::BexStr, Value> = IndexMap::new();
    for param in &sig.params {
        match param.mode {
            FunctionParamMode::Required => {
                let position = positional.len();
                let arg = alloc_arg(vm, param.name.as_ref(), position, param.ty.clone(), &defs);
                positional.push(arg);
            }
            FunctionParamMode::Optional => {
                // An optional parameter always has a source name; a nameless
                // one is unaddressable by callers (there is nothing to pass
                // it by), so it is simply absent from `opts`. Placeholders
                // are for positionals only and never enter by-name matching.
                if let Some(name) = &param.name {
                    let arg = alloc_arg(vm, Some(name), positional.len(), param.ty.clone(), &defs);
                    opts.insert(bex_str::BexStr::from(name.as_str()), arg);
                }
            }
        }
    }
    let args = Value::object(vm.tlab.alloc_array(ty_arg(), positional));
    let opts = Value::object(vm.tlab.alloc_map(RealizedTy::string(), ty_arg(), opts));
    let returns = alloc_type_in(vm, sig.ret.clone(), &defs);
    let errors = alloc_type_in(vm, sig.throws, &defs);
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

/// `reflect.call_any` mirrors the ordinary call boundary's one numeric
/// conversion: an exactly representable `int` may enter a `float` (or
/// `float?`) slot. Materialize the boxed float before dispatch so the callee
/// receives the runtime representation its signature promises.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the round trip deliberately detects lossy i64-to-f64 conversions"
)]
fn prepare_call_any_argument(vm: &mut BexVm, value: Value, expected: &RealizedTy) -> Option<Value> {
    fn is_float_slot(ty: &RealizedTy) -> bool {
        match ty {
            RealizedTy::Float { .. } => true,
            RealizedTy::Union(members, _) => {
                members.iter().any(is_float_slot)
                    && members
                        .iter()
                        .all(|member| member.is_null() || is_float_slot(member))
            }
            _ => false,
        }
    }

    if let bex_vm_types::ValueKind::Int(number) = value.kind()
        && is_float_slot(expected)
    {
        let widened = number as f64;
        if widened as i64 != number {
            return None;
        }
        return Some(Value::object(vm.tlab.alloc(Object::Float(widened))));
    }
    value_fits(vm, value, expected).then_some(value)
}

/// `reflect.call_any<R, E>(f, args) -> R throws E | InvalidArgumentError | CompilationError`.
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
        if let Some(name) = vm.unspecialized_generic_callable_name(f_val) {
            let diagnostic = runtime_type::unspecialized_reflected_generic(&name);
            return VmRustFnError::Thrown(super::type_kinds::alloc_compilation_error(
                vm,
                &[diagnostic],
            ))
            .into();
        }
        return non_callable_error("reflect.call_any").into();
    };
    // A generic whose signature happens to be free of its own type parameters
    // reconstructs above and would otherwise be entered with an empty frame,
    // failing inside its body as a VM internal error.
    if let Some(name) = vm.generic_callable_body_needs_type_args(f_val) {
        let diagnostic = runtime_type::unspecialized_reflected_generic_call(&name);
        return VmRustFnError::Thrown(super::type_kinds::alloc_compilation_error(
            vm,
            &[diagnostic],
        ))
        .into();
    }

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
                let Some(value) = prepare_call_any_argument(vm, value, &param.ty) else {
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
